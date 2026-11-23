use super::*;
#[test]
fn anonymous_account_marker_rejects_oauth_and_empty_library_without_rejecting_signed_in_shelf() {
    let signed_out = "<meta name='js_const_user_signed_in' content='false'>";
    let oauth = format!("{signed_out}<main><h1>ログイン</h1><form action='/users/auth/pixiv' method='post'><input type='submit' value='pixivアカウントでログイン'></form></main>");
    let empty = "<main><h1>Library</h1><div class='bg-white text-center'>No items</div></main>";
    for html in [oauth, format!("{signed_out}{empty}")] {
        assert_eq!(
            parse(&html, "library", "/library?page=1")
                .err()
                .unwrap()
                .to_string(),
            "AUTH_REQUIRED"
        );
    }
    for html in [
        empty.to_owned(),
        format!("<meta name='js_const_user_signed_in' content='true'>{empty}"),
    ] {
        let page = parse(&html, "library", "/library?page=1").unwrap();
        assert!(page.terminal);
        assert!(page.rows.is_empty());
    }
    assert_eq!(
        parse(
            "<main><h1>Unknown page</h1></main>",
            "library",
            "/library?page=1"
        )
        .err()
        .unwrap()
        .to_string(),
        "SOURCE_MARKER_UNKNOWN"
    );
}
fn charge_item(id: &str, file: &str, unit: &str, quantity: Option<&str>, boost: &str) -> String {
    let quantity = quantity
        .map(|value| format!("<div>{value}</div>"))
        .unwrap_or_default();
    format!("<div class='sheet' data-product-id='{id}'><section><div><div><b><a href='https://invented.booth.pm/items/{id}'>Authored {id}</a></b></div><div>{unit}</div>{quantity}<div><i class='icon-boost'></i><span>{boost}</span></div></div></section><div data-test='downloadable-list'><b data-test='filename'>authored-{file}.zip</b><a href='https://booth.pm/downloadables/{file}'>Download</a></div></div>")
}
fn charge_receipt(items: &str, total: &str, extra_summary: &str) -> String {
    format!("<main>{items}<div class='sheet'><div><div><span>注文番号</span><span>910001</span></div><div><span>注文日時</span><span>2026/10/10 09:30</span></div><div><span>お支払方法</span><span>PRIVATE_OPAQUE_METHOD</span></div><div><span>お支払金額</span><span>{total}</span></div>{extra_summary}</div></div></main>")
}
fn authored_notice(body: &str) -> String {
    format!("<div class='sheet'><div><div class='text-16 font-bold'>発送通知コメント</div><div class='text-14'>{body}</div></div></div>")
}
fn authored_contact(body: &str) -> String {
    format!("<div class='sheet'><div><p>お支払いが完了しました</p><a href='/conversations/authored_token?opaque=PRIVATE_QUERY'>ショップに連絡する</a><div>{body}</div></div></div>")
}
fn product_line_proof(html: &str, rows: &[Row], receipt_id: &str) -> Result<ProductLineProof> {
    product_line_proof_document(&Html::parse_document(html), rows, receipt_id)
}
fn assert_bound_product_price(page: &Page, amount: u64) {
    assert_eq!(
        page.rows
            .iter()
            .filter(|row| row.product_id.as_deref() == Some("900001"))
            .count(),
        1
    );
    for row in &page.rows {
        assert_eq!(
            row.product_line_total_jpy,
            (row.product_id.as_deref() == Some("900001")).then_some(amount)
        );
    }
}
#[tokio::test]
async fn parser_language_counts_received_pages_before_early_errors() {
    let cases = [
            ("<html lang='ja-JP'><head><meta name='js_const_user_signed_in' content='false'></head><body>PRIVATE_AUTH</body></html>", "AUTH_REQUIRED"),
            ("<html lang='EN_us'><body><main><h1>Library</h1><form id='challenge-form'></form></main></body></html>", "AUTH_CHALLENGE"),
            ("<html lang='ko-KR'><body><main><h1>PRIVATE_HEADING</h1></main></body></html>", "SOURCE_MARKER_UNKNOWN"),
            ("<html lang='zh-Hans'><body><main><h1>PRIVATE_HEADING</h1></main></body></html>", "SOURCE_MARKER_UNKNOWN"),
            ("<html lang='ZH_tw'><body><main><h1>PRIVATE_HEADING</h1></main></body></html>", "SOURCE_MARKER_UNKNOWN"),
            ("<html lang='zh-Hant'><body><main><h1>PRIVATE_HEADING</h1></main></body></html>", "SOURCE_MARKER_UNKNOWN"),
            ("<html lang='zh-Hant-PRIVATE_LANGUAGE'><body><main><h1>PRIVATE_HEADING</h1></main></body></html>", "SOURCE_MARKER_UNKNOWN"),
            ("<main><h1>Library</h1></main>", "NO_RECOGNIZED_ROWS"),
            ("<html lang='  '><body><main><h1>Library</h1></main></body></html>", "NO_RECOGNIZED_ROWS"),
        ];
    let (_, trace) = crate::support_trace::capture(async {
        for (html, expected) in cases {
            let error = parse(html, "library", "/library?page=1").unwrap_err();
            assert_eq!(error.to_string(), expected);
        }
    })
    .await;
    let counts = &trace["sources"]["library"];
    for (key, expected) in [
        ("parserCalls", 9),
        ("htmlJa", 1),
        ("htmlEn", 1),
        ("htmlKo", 1),
        ("htmlZhCn", 1),
        ("htmlZhTw", 2),
        ("htmlOther", 1),
        ("htmlUnknown", 2),
    ] {
        assert_eq!(counts[key], expected, "{key}");
    }
    assert_eq!(counts.as_object().unwrap().len(), 8);
    assert!(trace["financial"].as_object().unwrap().is_empty());
    for private in [
        "PRIVATE",
        "ja-JP",
        "EN_us",
        "ko-KR",
        "zh-Hans",
        "ZH_tw",
        "zh-Hant",
        "Library",
        "AUTH_REQUIRED",
    ] {
        assert!(!trace.to_string().contains(private), "{private}");
    }
}
#[tokio::test]
async fn receipt_trace_counts_only_current_parser_calls_without_source_values() {
    let item = charge_item("900001", "700001", "¥ 137", None, "0");
    let known = charge_receipt(&item, "¥ 137", "");
    let expected =
        serde_json::to_value(parse(&known, "purchase", "/orders/910001").unwrap()).unwrap();
    for language in [
        "ja-JP",
        "en-US",
        "ko-KR",
        "zh-Hans",
        "zh-Hant",
        "zh-Hant-PRIVATE_LANGUAGE",
    ] {
        let localized = format!("<html lang='{language}'><body>{known}</body></html>");
        assert_eq!(
            serde_json::to_value(parse(&localized, "purchase", "/orders/910001").unwrap()).unwrap(),
            expected
        );
    }
    let unknown_price = charge_receipt(
        &charge_item("900001", "700001", "PRIVATE_INVALID_PRICE", None, "0"),
        "¥ 137",
        "",
    );
    let cases = [
        format!("<html lang='ja-JP'><body>{known}</body></html>"),
        format!("<html lang='en-US'><body>{unknown_price}</body></html>"),
        format!("<html lang='zh-Hant-PRIVATE_LANGUAGE'><body><main>{item}</main></body></html>"),
        known.clone(),
        format!("<html lang='ja-JP'><body>{known}</body></html>"),
        format!("<html lang='ko-KR'><body>{known}</body></html>"),
        format!("<html lang='zh-Hans'><body>{known}</body></html>"),
        format!("<html lang='zh-Hant'><body>{known}</body></html>"),
    ];
    let (pages, trace) = crate::support_trace::capture(async {
        let pages = cases
            .iter()
            .map(|html| parse(html, "purchase", "/orders/910001").unwrap())
            .collect::<Vec<_>>();
        // A different source must not enter the purchase denominators.
        parse(HTML, "library", "/library?page=1").unwrap();
        pages
    })
    .await;
    for (index, page) in pages.iter().enumerate() {
        assert_eq!(page.rows.len(), 1);
        assert_eq!(
            page.rows[0].product_line_total_jpy,
            (index != 1).then_some(137)
        );
        assert_eq!(page.rows[0].acquired_date_text.is_some(), index != 2);
    }
    let facts = &trace["financial"];
    // The repeated receipt is intentionally another parser call, not a new
    // unique receipt or transaction. Missing price and date stay separate.
    for (key, expected) in [
        ("receiptPages", 8),
        ("rows", 8),
        ("rowsWithPaidProof", 7),
        ("rowsWithDateProof", 7),
        ("proofRejected", 0),
        ("digitalBoundItems", 8),
        ("uniqueMetadataItems", 8),
        ("unitPriceItems", 7),
        ("omittedQuantityItems", 8),
        ("explicitBoostItems", 8),
        ("productLineProofItems", 7),
        ("ambiguousItems", 1),
        ("digitalControlBindingUnprovenItems", 0),
        ("metadataShapeUnrecognizedItems", 0),
        ("unitPriceUnrecognizedItems", 1),
        ("quantityUnrecognizedItems", 0),
        ("boostUnrecognizedItems", 0),
        ("dateProofUnavailableRows", 1),
        ("htmlJa", 2),
        ("htmlEn", 1),
        ("htmlKo", 1),
        ("htmlZhCn", 1),
        ("htmlZhTw", 1),
        ("htmlOther", 1),
        ("htmlUnknown", 1),
    ] {
        assert_eq!(facts[key], expected, "{key}");
    }
    let serialized = trace.to_string();
    for private in [
        "PRIVATE",
        "900001",
        "700001",
        "910001",
        "https://",
        "booth.pm",
        "Authored",
        "2026/10/10",
        "¥",
        "137",
        "authored-700001.zip",
    ] {
        assert!(!serialized.contains(private), "source value {private}");
    }
    assert!(crate::support_trace::snapshot().is_none());
}
#[test]
fn unclosed_contact_envelopes_keep_the_bound_price() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let footer = authored_contact("PRIVATE_CUSTOMER");
    let mut cases = vec![
        (
            authored_contact(&"<span>PRIVATE</span>".repeat(25)),
            "structuralGuardRejected",
        ),
        (
            authored_contact(
                "<a href='https://PRIVATE.invalid/SECRET_BODY_ROUTE'>PRIVATE_OTHER_LINK</a>",
            ),
            "rivalPresenceRejected",
        ),
        (
            footer
                .replace("<div class='sheet'><div>", "<div class='sheet'><section>")
                .replace("</div></div></div>", "</div></section></div>"),
            "envelopeStructureRejected",
        ),
        (
            footer.replace("お支払いが完了しました", "PRIVATE_UNKNOWN_STATUS"),
            "instructionRejected",
        ),
        (
            authored_contact("<span>PRIVATE_CUSTOMER</span>"),
            "opaqueBodyStructureRejected",
        ),
        (footer.clone(), "closedContacts"),
        (
            footer.replace(
                "<div>PRIVATE_CUSTOMER</div>",
                "<div data-discount='100'>PRIVATE_CUSTOMER</div>",
            ),
            "rivalPresenceRejected",
        ),
        (
            footer.replace(
                "<div>PRIVATE_CUSTOMER</div>",
                "<div data-fee='100'>PRIVATE_CUSTOMER</div>",
            ),
            "rivalPresenceRejected",
        ),
        (
            footer.replace("<div class='sheet'>", "<div class='sheet'>PRIVATE_OUTSIDE"),
            "envelopeStructureRejected",
        ),
    ];
    cases.extend(
        [
            "data-id",
            "data-item",
            "data-product",
            "data-product-id",
            "data-item-id",
        ]
        .map(|attribute| {
            (
                footer.replace(
                    "<div class='sheet'>",
                    &format!("<div class='sheet' {attribute}='900003'>"),
                ),
                "rivalPresenceRejected",
            )
        }),
    );
    cases.extend(
        [
            footer.replace("class='sheet'", "class='sheet icon-boost'"),
            footer.replace(
                "<div class='sheet'>",
                "<div class='sheet' data-test='filename'>",
            ),
        ]
        .map(|candidate| (candidate, "rivalPresenceRejected")),
    );
    for (footer, expected) in cases {
        if expected != "closedContacts" {
            let page = html.replace("</main>", &format!("{footer}</main>"));
            assert_bound_product_price(&parse(&page, "purchase", "/orders/910001").unwrap(), 100);
        }
    }
}
#[test]
fn contact_customer_text_values_never_change_dates_or_proven_amounts() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let bodies = [
        "PRIVATE_CUSTOMER".to_owned(),
        "PRIVATE".repeat(200),
        "  \n\t  ".into(),
        "注文番号 910001 注文日時 1999/01/01 01:01 お支払金額 ¥ 777777".into(),
        "".into(),
        "PRIVATE<!--PRIVATE_COMMENT-->OTHER_PRIVATE_TEXT".into(),
    ];
    let mut outcomes = Vec::new();
    for body in bodies {
        let footer = authored_contact(&body);
        let page = html.replace("</main>", &format!("{footer}</main>"));
        let parsed = parse(&page, "purchase", "/orders/910001").unwrap();
        let row = parsed
            .rows
            .iter()
            .find(|row| row.product_id.as_deref() == Some("900001"))
            .unwrap();
        assert_eq!(row.product_line_total_jpy, Some(100));
        assert_eq!(row.acquired_date_text.as_deref(), Some("2026/10/10 09:30"));
        outcomes.push(row.source_record_id.clone());
    }
    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}
#[test]
fn known_contact_opaque_body_is_excluded_before_metadata_links_files_and_legacy_labels() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let saved = parse(&html, "purchase", "/orders/910001").unwrap().rows;
    let labels = "<div><span>注文番号</span><span>910001</span></div><div><span>注文日時</span><span>1999/01/01 01:01</span></div><div><span>お支払金額</span><span>¥ 777777</span></div>";
    let footer = authored_contact(labels);
    for candidate in [
            footer.clone(),
            authored_contact(&format!("<h1>PRIVATE_BODY_HEADING</h1>{labels}")),
            authored_contact(&format!("<h2>PRIVATE_BODY_SUBHEADING</h2><div data-test='page-title'>PRIVATE_BODY_TITLE</div>{labels}")),
            footer.replace("<div class='sheet'><div>", "<div class='sheet'><section>").replace("</div></div></div>", "</div></section></div>"),
            authored_contact(&format!("{labels}<a href='https://PRIVATE.invalid/PRIVATE_BODY_URI'>PRIVATE_BODY_ACTION</a>")),
            authored_contact(&format!("{labels}<a href='https://invented.booth.pm/items/900002'>PRIVATE_FAKE_PRODUCT</a><div data-test='downloadable-list'><b data-test='filename'>PRIVATE.zip</b><a href='https://booth.pm/downloadables/700002'>Download</a></div>")),
        ] {
            let fragment = Html::parse_fragment(&format!("<section data-test='wrapper'>{candidate}</section>"));
            let card = fragment.select(&sel(".sheet")).next().unwrap();
            let body = known_contact_prefix(card).unwrap();
            let wrapper = fragment.select(&sel("[data-test='wrapper']")).next().unwrap();
            assert!(!rebound_product_fields(card, &saved));
            for node in [card, wrapper, body] {
                assert!(!template_summary_candidate(node));
                let observation = receipt_summary_observation(node, "910001");
                assert_eq!(observation.number_labels, 0);
                assert_eq!(observation.date_labels, 0);
                assert_eq!(observation.date, None);
            }
            let before = html.replace("<main>", &format!("<main>{candidate}"));
            let parsed = parse(&before, "purchase", "/orders/910001").unwrap();
            assert_eq!(parsed.rows.iter().filter(|row| row.product_id.is_some()).count(), 1);
            assert_eq!(parsed.rows.len(), 1);
            assert_eq!(parsed.rows[0].files.len(), 1);
            assert_eq!(parsed.rows[0].files[0].id, "700001");
            assert_eq!(parsed.rows[0].product_id.as_deref(), Some("900001"));
            assert!(parsed.rows[0].name.as_deref().is_some_and(|name| !name.contains("PRIVATE")));
            assert!(parsed.rows[0].paid_amount_text.is_none());
            assert!(!serde_json::to_string(&parsed).unwrap().contains("PRIVATE"));
            let digital = parsed.rows.iter().find(|row| row.product_id.as_deref() == Some("900001")).unwrap();
            assert_eq!(digital.acquired_date_text.as_deref(), Some("2026/10/10 09:30"));
            assert_eq!(digital.product_line_total_jpy, Some(100));
            let no_summary = parse(&format!("<main>{candidate}{item}</main>"), "purchase", "/orders/910001").unwrap();
            assert!(no_summary.rows.iter().all(|row| row.acquired_date_text.is_none()));
            assert_bound_product_price(&no_summary, 100);
        }
    let body_link_only = "<div class='sheet'><div class='text-14'><p>お支払いが完了しました</p><a href='/conversations/PRIVATE_TOKEN'>ショップに連絡する</a><div>PRIVATE_BODY</div></div></div>";
    let fragment = Html::parse_fragment(body_link_only);
    assert!(known_contact_prefix(fragment.select(&sel(".sheet")).next().unwrap()).is_none());
}
#[test]
fn unclosed_notice_envelopes_keep_the_bound_price() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let header = "<div class='text-16 font-bold'>発送通知コメント</div>";
    let body = "<div class='text-14'><p>PRIVATE_NOTICE_BODY 777777</p></div>";
    let notice = authored_notice("<p>PRIVATE_NOTICE_BODY 777777</p>");
    let cases = vec![
            (
                authored_notice(&"<span>PRIVATE</span>".repeat(70)),
                "structuralGuardRejected",
            ),
            (
                notice.replace("<p>", "<p data-id='900003'>"),
                "rivalPresenceRejected",
            ),
            (
                format!("<section class='sheet'>{header}{body}</section>"),
                "outerWrappingRejected",
            ),
            (
                format!(
                    "<div class='sheet'><div>{header}{body}<div>PRIVATE_EXTRA</div></div></div>"
                ),
                "siblingStructureRejected",
            ),
            (
                "<div class='sheet'><b>発送通知コメント</b><p>PRIVATE<br>PRIVATE<br></p></div>".to_string(),
                "bodyRoleRejected",
            ),
            (
                "<div class='sheet'><b>発送通知コメント</b><p>PRIVATE<br>PRIVATE<br>PRIVATE<br>PRIVATE<br></p></div>".to_string(),
                "bodyRoleRejected",
            ),
            (authored_notice("<em>PRIVATE</em>"), "bodyTypeRejected"),
            (
                authored_notice(&format!(
                    "{}PRIVATE{}",
                    "<div>".repeat(8),
                    "</div>".repeat(8)
                )),
                "bodyDepthRejected",
            ),
            (notice.clone(), "closedNotices"),
            (
                authored_notice(&format!("{header}<p>PRIVATE_DUPLICATE</p>")),
                "closedNotices",
            ),
            (
                format!("<div class='sheet'><div>{header}{header}</div></div>"),
                "closedNotices",
            ),
            (
                format!("<div class='sheet'><div>{header}{header}{body}</div></div>"),
                "siblingStructureRejected",
            ),
            (
                notice.replace("<div class='text-14'>", "PRIVATE_OUTSIDE<div class='text-14'>"),
                "siblingStructureRejected",
            ),
        ];
    for (notice, expected) in cases {
        if expected != "closedNotices" {
            let page = html.replace("</main>", &format!("{notice}</main>"));
            assert_bound_product_price(&parse(&page, "purchase", "/orders/910001").unwrap(), 100);
        }
    }
}
#[test]
fn unsupported_and_sibling_duplicate_notice_headers_omit_financial_date_nomination() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let body = "<div><span>注文番号</span><span>910001</span></div><div><span>注文日時</span><span>1999/01/01 01:01</span></div><div><span>お支払金額</span><span>¥ 777777</span></div><p>PRIVATE_OPAQUE</p>";
    let valid = authored_notice(body);
    for notice in [
            valid.replace("<p>", "<p data-discount='100'>"),
            valid.replace("<div class='sheet'>", "<div class='sheet icon-boost'>"),
            valid.replace("<p>", "<em>").replace("</p>", "</em>"),
            valid.replace("<div class='text-16 font-bold'>発送通知コメント</div>", "<div class='text-16 font-bold'>発送通知コメント</div><div class='text-16 font-bold'>発送通知コメント</div>"),
        ] {
            let wrapped =
                Html::parse_fragment(&format!("<section data-test='wrapper'>{notice}</section>"));
            let card = wrapped.select(&sel(".sheet")).next().unwrap();
            let wrapper = wrapped
                .select(&sel("[data-test='wrapper']"))
                .next()
                .unwrap();
            let direct_body = card.select(&sel("p,em")).next().unwrap();
            for candidate in [card, wrapper, direct_body] {
                assert!(!template_summary_candidate(candidate));
                let observation = receipt_summary_observation(candidate, "910001");
                assert_eq!(observation.number_labels, 0);
                assert_eq!(observation.date_labels, 0);
                assert_eq!(observation.date, None);
                assert!(!observation.role);
            }
            let before = html.replace("<main>", &format!("<main>{notice}"));
            let parsed = parse(&before, "purchase", "/orders/910001").unwrap();
            let digital = parsed
                .rows
                .iter()
                .find(|row| row.product_id.as_deref() == Some("900001"))
                .unwrap();
            assert_eq!(
                digital.acquired_date_text.as_deref(),
                Some("2026/10/10 09:30")
            );
            assert_eq!(digital.product_line_total_jpy, Some(100));
            let no_summary = parse(
                &format!("<main>{notice}{item}</main>"),
                "purchase",
                "/orders/910001",
            )
            .unwrap();
            assert!(no_summary.rows.iter().all(|row| row.acquired_date_text.is_none()));
            assert_bound_product_price(&no_summary, 100);
        }
}
#[test]
fn notice_semantic_heading_and_known_p_body_preserve_proven_facts() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let mut snapshots = Vec::new();
    let body = "<div>PRIVATE ¥ 777777</div><div class='text-16 font-bold'>発送通知コメント</div><p>PRIVATE</p>";
    for notice in [
            authored_notice(body),
            authored_notice(body).replace("text-16 font-bold", "text-16"),
            authored_notice(body).replace("class='text-14'", "class='unknown-body'"),
            format!("<div class='sheet'><div>発送通知コメント</div><div>{body}</div></div>"),
            format!("<div class='sheet'><span><div><span><b><strong>発送通知コメント</strong></b></span></div><div>{body}</div></span></div>"),
            format!("<div class='sheet'><div><h1><span>発送通知コメント</span></h1><div>{body}</div></div></div>"),
            format!("<div class='sheet'><div><h2>発送通知コメント</h2><div>{body}</div></div></div>"),
            "<div class='sheet'><b>発送通知コメント</b><p>PRIVATE<br>PRIVATE<br>PRIVATE<br>PRIVATE</p></div>".to_string(),
            "<div class='sheet'><span><b>発送通知コメント</b><p>PRIVATE<br><span>PRIVATE<br>PRIVATE<br></span></p></span></div>".to_string(),
        ] {
            let page = html.replace("</main>", &format!("{notice}</main>"));
            let parsed = parse(&page, "purchase", "/orders/910001").unwrap();
            let product = parsed.rows.iter().find(|row| row.product_id.as_deref() == Some("900001")).unwrap();
            assert_eq!(product.product_line_total_jpy, Some(100));
            assert_eq!(product.acquired_date_text.as_deref(), Some("2026/10/10 09:30"));
            snapshots.push((product.source_record_id.clone(),
                product.acquired_date_text.clone(), product.product_line_total_jpy));
        }
    assert!(snapshots.iter().all(|snapshot| snapshot == &snapshots[0]));
}
#[test]
fn notice_prefix_never_retries_unknown_or_text_bearing_body_wrappers() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let header = "<b>発送通知コメント</b>";
    let pair = format!("{header}<div>PRIVATE</div>");
    let mut cases: Vec<_> = [
            "<div class='sheet'><div class='text-14'><div>発送通知コメント</div><div>PRIVATE</div></div></div>".to_string(),
            format!("<div class='sheet'><div>PRIVATE_BODY<div>{pair}</div></div></div>"),
            format!("<div class='sheet'><div> <div>{pair}</div></div></div>"),
            format!("<div class='sheet'><div><span>PRIVATE_UNKNOWN_HEADING</span><div>{pair}</div></div></div>"),
            format!("<div class='sheet'><div><div>{pair}</div><div>PRIVATE</div></div></div>"),
            format!("<div class='sheet'><div><span>PRIVATE_UNKNOWN_HEADING</span><div>PRIVATE</div>{pair}</div></div>"),
            format!("<div class='sheet'><section>{pair}</section></div>"),
            "<div class='sheet'><div>発送通知コメント</div><p>PRIVATE<br><br><br></p></div>".to_string(),
            "<div class='sheet'><b><span>発送通知コメント</span></b><p>PRIVATE<br><br><br></p></div>".to_string(),
            format!("<div class='sheet'>{header}<section>PRIVATE<br><br><br></section></div>"),
            "<div class='sheet'><b data-id='900003'>発送通知コメント</b><div>PRIVATE</div></div>".to_string(),
            // Relative links resolve to the account host, which is not an item host.
            "<div class='sheet'><b><a href='/items/900003'>発送通知コメント</a></b><div>PRIVATE</div></div>".to_string(),
        ].into_iter().collect();
    cases.push("<div class='sheet'><b><a href='https://booth.pm/ja/items/900003'>発送通知コメント</a></b><div>PRIVATE</div></div>".to_string());
    for notice in cases {
        let fragment = Html::parse_fragment(&notice);
        assert!(known_notice_prefix(fragment.select(&sel(".sheet")).next().unwrap()).is_none());
        let page = html.replace("</main>", &format!("{notice}</main>"));
        assert_bound_product_price(&parse(&page, "purchase", "/orders/910001").unwrap(), 100);
    }
}
#[test]
fn opaque_styled_notice_body_never_supplies_a_header_or_changes_proven_facts() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    let mut positive = Vec::new();
    for literal in ["PRIVATE_OPAQUE_STYLED_BODY", "発送通知コメント"] {
        let body = format!(
            "<div class='text-16 font-bold'>{literal}</div><p>PRIVATE_BODY_TEXT ¥ 777777</p>"
        );
        let notice = authored_notice(&body);
        let page = html.replace("</main>", &format!("{notice}</main>"));
        let parsed = parse(&page, "purchase", "/orders/910001").unwrap();
        let product = parsed
            .rows
            .iter()
            .find(|row| row.product_id.as_deref() == Some("900001"))
            .unwrap();
        assert_eq!(product.product_line_total_jpy, Some(100));
        assert_eq!(
            product.acquired_date_text.as_deref(),
            Some("2026/10/10 09:30")
        );
        positive.push(product.source_record_id.clone());
        let body_first = format!("<div class='sheet'><div class='text-14'>{body}</div></div>");
        let fragment = Html::parse_fragment(&body_first);
        let card = fragment.select(&sel(".sheet")).next().unwrap();
        assert!(known_notice_header(card).is_none());
        let page = html.replace("</main>", &format!("{body_first}</main>"));
        assert_bound_product_price(&parse(&page, "purchase", "/orders/910001").unwrap(), 100);
        let body_before_header = format!("<div class='sheet'><div><div class='text-14'>{body}</div><div class='text-16 font-bold'>発送通知コメント</div></div></div>");
        let fragment = Html::parse_fragment(&body_before_header);
        assert!(known_notice_header(fragment.select(&sel(".sheet")).next().unwrap()).is_none());
    }
    assert_eq!(positive[0], positive[1]);
}
#[test]
fn independent_product_fields_ignore_unrelated_ui_and_infer_only_known_digital_quantity() {
    let explicit = charge_item("900001", "700001", "¥ 100", Some("数量: 2"), "30").replace(
        "</section>",
        "</section><button>PRIVATE_UNRELATED_ACTION</button><p>PRIVATE_UNRELATED_TEXT</p>",
    );
    let omitted = charge_item("900001", "700001", "¥ 100", None, "30");
    let unknown = "<div class='sheet'><p>PRIVATE_UNKNOWN_UI ¥ 999999</p></div><nav><button>PRIVATE_NAV</button></nav>";
    for (item, amount, explicit_count, omitted_count) in
        [(explicit, 230, 1, 0), (omitted, 130, 0, 1)]
    {
        let html = format!("<main>{item}{unknown}</main>");
        let page = parse(&html, "purchase", "/orders/910001").unwrap();
        assert_bound_product_price(&page, amount);
        assert!(page.rows.iter().all(|row| row.acquired_date_text.is_none()));
        let proof = product_line_proof(&html, &page.rows, "910001").unwrap();
        assert_eq!(proof.charges.len(), 1);
        assert_eq!(proof.charges[0].amount, amount);
        assert_eq!(proof.diagnostics.explicit_quantity_items, explicit_count);
        assert_eq!(proof.diagnostics.omitted_quantity_items, omitted_count);
        let counts = serde_json::to_value(&proof.diagnostics).unwrap();
        let keys = [
            "version",
            "digitalBoundItems",
            "uniqueMetadataItems",
            "unitPriceItems",
            "explicitQuantityItems",
            "omittedQuantityItems",
            "explicitBoostItems",
            "productLineProofItems",
            "ambiguousItems",
            "digitalControlBindingUnprovenItems",
            "metadataShapeUnrecognizedItems",
            "unitPriceUnrecognizedItems",
            "quantityUnrecognizedItems",
            "boostUnrecognizedItems",
        ];
        assert_eq!(counts.as_object().unwrap().len(), keys.len());
        assert!(counts
            .as_object()
            .unwrap()
            .keys()
            .all(|key| keys.contains(&key.as_str())));
        assert_eq!(counts["version"], 1);
        assert_eq!(counts["digitalBoundItems"], 1);
        assert_eq!(counts["uniqueMetadataItems"], 1);
        assert_eq!(counts["unitPriceItems"], 1);
        assert_eq!(counts["explicitBoostItems"], 1);
        assert_eq!(counts["productLineProofItems"], 1);
        assert_eq!(counts["ambiguousItems"], 0);
        for key in [
            "digitalControlBindingUnprovenItems",
            "metadataShapeUnrecognizedItems",
            "unitPriceUnrecognizedItems",
            "quantityUnrecognizedItems",
            "boostUnrecognizedItems",
        ] {
            assert_eq!(counts[key], 0, "{key}");
        }
        assert!(counts
            .as_object()
            .unwrap()
            .values()
            .all(|value| value.is_u64()));
        for private in [
            "PRIVATE", "999999", "900001", "700001", "910001", "https://", "Authored",
        ] {
            assert!(!counts.to_string().contains(private));
        }
        let changed = html.replace(
            "PRIVATE_UNKNOWN_UI ¥ 999999",
            "PRIVATE_OTHER_UNRELATED_VALUE",
        );
        assert_eq!(
            serde_json::to_value(
                &product_line_proof(&changed, &page.rows, "910001")
                    .unwrap()
                    .diagnostics
            )
            .unwrap(),
            counts
        );
    }
}
#[test]
fn five_locale_receipts_bind_english_summary_without_changing_acquisition_identity() {
    let japanese = charge_receipt(
        &charge_item("900001", "700001", "¥ 137", None, "¥ 30"),
        "¥ 999999",
        "",
    )
    .replace("2026/10/10 09:30", "2026/10/10 09:30:12");
    let baseline = parse(&japanese, "purchase", "/orders/910001").unwrap();
    let international = japanese
        .replace("¥ 137", "137 JPY")
        .replace("¥ 30", "30 JPY")
        .replace("¥ 999999", "999999 JPY")
        .replace("注文番号", "Order Number")
        .replace("注文日時", "Created At");
    for (language, receipt) in [
        ("ja", &japanese),
        ("en", &international),
        ("ko", &international),
        ("zh-CN", &international),
        ("zh-TW", &international),
    ] {
        let html = format!("<html lang='{language}'><body>{receipt}</body></html>");
        let page = parse(&html, "purchase", "/orders/910001").unwrap();
        assert_bound_product_price(&page, 167);
        assert_eq!(
            page.rows[0].acquired_date_text.as_deref(),
            Some("2026/10/10 09:30:12")
        );
        assert_eq!(
            serde_json::to_value(page).unwrap(),
            serde_json::to_value(&baseline).unwrap()
        );
    }
    for receipt in [
            international.replace("<span>910001</span>", "<span>910002</span>"),
            international.replace("Created At", "PRIVATE_UNKNOWN_DATE_LABEL"),
            international.replace("<span>Created At</span>", "<span>Created At</span><span>Created At</span>"),
            international.replace("2026/10/10 09:30", "2026/13/10 09:30"),
            international.replace("</main>", "<div class='sheet'><div><span>Order Number</span><span>910001</span></div><div><span>Created At</span><span>2026/10/11 09:30</span></div></div></main>"),
        ] {
            let page = parse(&receipt, "purchase", "/orders/910001").unwrap();
            assert_bound_product_price(&page, 167);
            assert_eq!(page.rows[0].acquired_date_text, None);
            assert_eq!(page.rows[0].source_record_id, baseline.rows[0].source_record_id);
            assert_eq!(page.rows[0].product_id, baseline.rows[0].product_id);
            assert_eq!(serde_json::to_value(&page.rows[0].files).unwrap(), serde_json::to_value(&baseline.rows[0].files).unwrap());
        }
}
#[test]
fn receipt_jpy_code_preserves_bound_totals_and_rejects_other_amount_spellings() {
    for (quantity, amount) in [(None, 167), (Some("Quantity: 2"), 304)] {
        let japanese = charge_receipt(
            &charge_item("900001", "700001", "¥ 137", quantity, "¥ 30"),
            "¥ 999999",
            "",
        );
        let international = charge_receipt(
            &charge_item("900001", "700001", "137 JPY", quantity, "30 JPY"),
            "999999 JPY",
            "",
        );
        let expected = parse(&japanese, "purchase", "/orders/910001").unwrap();
        let parsed = parse(&international, "purchase", "/orders/910001").unwrap();
        assert_bound_product_price(&parsed, amount);
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    for (unit, boost) in [
        ("137 USD", "30 JPY"),
        ("137 JPY", "30 USD"),
        ("¥ 137 JPY", "30 JPY"),
        ("137 JPY", ""),
        ("137.0 JPY", "30 JPY"),
        ("137 JPY ~ 999 JPY", "30 JPY"),
    ] {
        let html = charge_receipt(
            &charge_item("900001", "700001", unit, None, boost),
            "999999 JPY",
            "",
        );
        let page = parse(&html, "purchase", "/orders/910001").unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].product_line_total_jpy, None);
        assert_eq!(
            page.rows[0].acquired_date_text.as_deref(),
            Some("2026/10/10 09:30")
        );
        assert_eq!(page.rows[0].files.len(), 1);
    }
}
#[test]
fn product_field_rejection_counts_describe_existing_decisions_without_source_values() {
    let item = charge_item("900001", "700001", "¥ 100", Some("Quantity: 2"), "30");
    let html = charge_receipt(&item, "¥ 230", "");
    let saved = parse(&html, "purchase", "/orders/910001").unwrap().rows;
    let reasons = [
        "digitalControlBindingUnprovenItems",
        "metadataShapeUnrecognizedItems",
        "unitPriceUnrecognizedItems",
        "quantityUnrecognizedItems",
        "boostUnrecognizedItems",
    ];
    for (changed, expected_reason) in [
        (html.replace("700001", "700002"), reasons[0]),
        (
            html.replace(
                "<div>¥ 100</div>",
                "<div>¥ 100</div><div>PRIVATE_EXTRA_FIELD</div>",
            ),
            reasons[1],
        ),
        (
            html.replace("<div>¥ 100</div>", "<div>PRIVATE_UNIT_VALUE</div>"),
            reasons[2],
        ),
        (
            html.replace("Quantity: 2", "PRIVATE_QUANTITY_VALUE"),
            reasons[3],
        ),
        (
            html.replace("<span>30</span>", "<span>PRIVATE_BOOST_VALUE</span>"),
            reasons[4],
        ),
    ] {
        let proof = product_line_proof(&changed, &saved, "910001").unwrap();
        assert!(proof.charges.is_empty());
        assert_eq!(proof.diagnostics.ambiguous_items, 1);
        let counts = serde_json::to_value(&proof.diagnostics).unwrap();
        for reason in reasons {
            assert_eq!(
                counts[reason],
                usize::from(reason == expected_reason),
                "{reason}"
            );
        }
        for private in [
            "PRIVATE", "900001", "700001", "700002", "910001", "https://", "Authored", "¥",
        ] {
            assert!(!counts.to_string().contains(private), "{private}");
        }
    }
    let mut unavailable = saved;
    unavailable
        .iter_mut()
        .find(|row| row.product_id.is_some())
        .unwrap()
        .unavailable = true;
    let proof = product_line_proof(&html, &unavailable, "910001").unwrap();
    assert!(proof.charges.is_empty());
    assert_eq!(proof.diagnostics.digital_control_binding_unproven_items, 1);
    // Eligibility failures did not previously enter the ambiguous-item denominator.
    assert_eq!(proof.diagnostics.ambiguous_items, 0);
}
#[test]
fn independent_product_fields_reject_bad_needed_values_and_duplicate_binding() {
    let item = charge_item("900001", "700001", "¥ 100", Some("数量: 2"), "30");
    let html = charge_receipt(&item, "¥ 230", "");
    let saved = parse(&html, "purchase", "/orders/910001").unwrap().rows;
    for changed in [
        item.replace("¥ 100", "USD 100"),
        item.replace("¥ 100", "¥ 100 - 20"),
        item.replace("¥ 100", "¥ 9007199254740992"),
        item.replace("¥ 100", "¥ 9007199254740991"),
        item.replace("数量: 2", "数量: 0"),
        item.replace("数量: 2", "数量: -2"),
        item.replace("数量: 2", "数量: 2.5"),
        item.replace("数量: 2", "PRIVATE_UNKNOWN_QUANTITY"),
        item.replace("<i class='icon-boost'></i>", ""),
        item.replace("<span>30</span>", "<span>PRIVATE 30</span>"),
        item.replace("<div>¥ 100</div>", "<div data-discount='20'>¥ 100</div>"),
        item.replace("<div>¥ 100</div>", "<div>¥ 100</div><div>¥ 999</div>"),
        item.replace(
            "</b></div>",
            "</b><b><a href='https://invented.booth.pm/items/900001'>Authored 900001</a></b></div>",
        ),
    ] {
        let page = parse(
            &format!("<main>{changed}</main>"),
            "purchase",
            "/orders/910001",
        )
        .unwrap();
        assert!(page
            .rows
            .iter()
            .all(|row| row.product_line_total_jpy.is_none()));
        assert!(
            product_line_proof(&format!("<main>{changed}</main>"), &page.rows, "910001")
                .unwrap()
                .charges
                .is_empty()
        );
    }
    let mut duplicate = saved.clone();
    duplicate.push(
        saved
            .iter()
            .find(|row| row.product_id.is_some())
            .unwrap()
            .clone(),
    );
    assert!(product_line_proof(&html, &duplicate, "910001")
        .unwrap()
        .charges
        .is_empty());
    let stale = html.replace("700001", "700002");
    assert!(product_line_proof(&stale, &saved, "910001")
        .unwrap()
        .charges
        .is_empty());
    assert!(product_line_proof(&html, &saved, "910002")
        .unwrap()
        .charges
        .is_empty());
    for case in 0..3 {
        let mut invalid = saved.clone();
        let row = invalid
            .iter_mut()
            .find(|row| row.product_id.is_some())
            .unwrap();
        match case {
            0 => row.files[0].id = "700099".into(),
            1 => row.unavailable = true,
            2 => row.error_code = Some("FILE_INCOMPLETE".into()),
            _ => unreachable!(),
        }
        assert!(product_line_proof(&html, &invalid, "910001")
            .unwrap()
            .charges
            .is_empty());
    }
}
#[test]
fn independent_product_fields_keep_zero_and_only_proven_products() {
    for boost in ["0", "30"] {
        let html = format!(
            "<main>{}</main>",
            charge_item("900001", "700001", "¥ 0", None, boost)
        );
        assert_bound_product_price(
            &parse(&html, "purchase", "/orders/910001").unwrap(),
            boost.parse().unwrap(),
        );
    }
    let first = charge_item("900001", "700001", "¥ 100", Some("数量: 2"), "30");
    let second = charge_item(
        "900002",
        "700002",
        "PRIVATE_BAD_UNIT",
        Some("数量: 2"),
        "30",
    );
    let html = format!("<main>{first}{second}</main>");
    let page = parse(&html, "purchase", "/orders/910001").unwrap();
    assert_eq!(
        page.rows
            .iter()
            .filter(|row| row.product_id.is_some())
            .map(|row| row.product_line_total_jpy)
            .collect::<Vec<_>>(),
        vec![Some(230), None]
    );
    let duplicate_product = format!("<main>{first}{}</main>", first.replace("700001", "700003"));
    assert!(parse(&duplicate_product, "purchase", "/orders/910001")
        .unwrap()
        .rows
        .iter()
        .all(|row| row.product_line_total_jpy.is_none()));
    let conflicting_file = format!(
        "<main>{first}{}</main>",
        charge_item("900002", "700001", "¥ 100", None, "30")
    );
    assert!(parse(&conflicting_file, "purchase", "/orders/910001")
        .unwrap()
        .rows
        .iter()
        .all(|row| row.product_line_total_jpy.is_none()));
}
#[test]
fn independent_product_fields_bind_multiple_exact_owned_variants_without_amount_allocation() {
    let first = charge_item("900001", "700001", "¥ 100", Some("数量: 2"), "30").replace(
        "/downloadables/700001'",
        "/downloadables/700001?variation_id=800001'",
    );
    let second = charge_item("900001", "700002", "¥ 0", None, "0").replace(
        "/downloadables/700002'",
        "/downloadables/700002?variation_id=800002'",
    );
    let page = parse(
        &format!("<main>{first}{second}</main>"),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    assert_eq!(page.rows.len(), 2);
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.product_line_total_jpy)
            .collect::<Vec<_>>(),
        vec![Some(230), Some(0)]
    );
    assert_ne!(page.rows[0].source_record_id, page.rows[1].source_record_id);
    let ambiguous = second.replace("800002", "800001");
    assert!(parse(
        &format!("<main>{first}{ambiguous}</main>"),
        "purchase",
        "/orders/910001"
    )
    .unwrap()
    .rows
    .iter()
    .all(|row| row.product_line_total_jpy.is_none()));
}
#[test]
fn needed_financial_fields_survive_missing_title_with_exact_digital_binding() {
    for (unit, quantity, boost, amount) in
        [("¥ 0", None, "0", 0), ("¥ 100", Some("数量: 2"), "30", 230)]
    {
        let item =
            charge_item("900001", "700001", unit, quantity, boost).replace("Authored 900001", "");
        let html = charge_receipt(&item, &format!("¥ {amount}"), "");
        let page = parse(&html, "purchase", "/orders/910001").unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.unresolved, 1);
        let row = &page.rows[0];
        assert!(row.name.is_none());
        assert_eq!(row.error_code.as_deref(), Some("PRODUCT_TITLE_UNRESOLVED"));
        assert!(!row.manifest_complete);
        assert!(product_field_file_keys(row).is_some());
        assert_eq!(row.acquired_date_text.as_deref(), Some("2026/10/10 09:30"));
        assert_eq!(row.product_line_total_jpy, Some(amount));
        let mut invalid = row.clone();
        for code in ["PRODUCT_ID_UNRESOLVED", "DIGITAL_CONTROLS_UNRESOLVED"] {
            invalid.error_code = Some(code.into());
            assert!(product_field_file_keys(&invalid).is_none());
        }
        invalid.error_code = row.error_code.clone();
        invalid.unavailable = true;
        assert!(product_field_file_keys(&invalid).is_none());
        let duplicate = charge_receipt(
            &format!("{item}{}", item.replace("700001", "700002")),
            &format!("¥ {}", amount * 2),
            "",
        );
        assert!(parse(&duplicate, "purchase", "/orders/910001")
            .unwrap()
            .rows
            .iter()
            .all(|row| row.product_line_total_jpy.is_none()));
    }
}
#[test]
fn needed_fields_survive_missing_filename_and_unrelated_receipt_regions() {
    let item = charge_item("900001", "700001", "¥ 100", Some("数量: 2"), "30");
    let complete_html = charge_receipt(&item, "¥ 230", "");
    let complete = parse(&complete_html, "purchase", "/orders/910001").unwrap();
    let html = complete_html
        .replace("<b data-test='filename'>authored-700001.zip</b>", "")
        .replace(
            "</main>",
            "<div class='sheet'><p>UNRELATED_UNKNOWN_REGION ¥ 999999</p></div></main>",
        );
    let page = parse(&html, "purchase", "/orders/910001").unwrap();
    let row = &page.rows[0];
    assert_eq!(page.unresolved, 0);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(row.source_record_id, complete.rows[0].source_record_id);
    assert_eq!(row.product_line_total_jpy, Some(230));
    assert_eq!(row.acquired_date_text.as_deref(), Some("2026/10/10 09:30"));
    assert!(!row.manifest_complete);
    assert!(product_field_file_keys(row).is_some());
    let broken = html.replace("</main>", "<div class='sheet'><a data-test='downloadable' href='https://evil.test/downloadables/700003'>Download</a></div></main>");
    let broken_page = parse(&broken, "purchase", "/orders/910001").unwrap();
    assert!(broken_page.unresolved > 0);
    assert_eq!(broken_page.rows[0].product_line_total_jpy, Some(230));
    assert_eq!(
        broken_page.rows[0].acquired_date_text.as_deref(),
        Some("2026/10/10 09:30")
    );
    let unknown = broken_page
        .rows
        .iter()
        .find(|row| row.product_id.is_none())
        .unwrap();
    assert_eq!(unknown.order_id.as_deref(), Some("910001"));
    assert_eq!(
        unknown.error_code.as_deref(),
        Some("DIGITAL_CONTROLS_UNRESOLVED")
    );
    assert!(
        unknown.name.is_none() && unknown.paid_amount_text.is_none() && unknown.files.is_empty()
    );
    let mixed = html.replace("<div data-test='downloadable-list'>", "<div data-test='downloadable-list'><a href='https://evil.test/downloadables/700003'>Download</a>");
    let mixed_page = parse(&mixed, "purchase", "/orders/910001").unwrap();
    assert!(mixed_page.unresolved > 0);
    assert!(mixed_page.rows[0].product_line_total_jpy.is_none());
}
#[test]
fn native_paid_producer_joins_exact_rows_including_zero_without_changing_identity() {
    let items = format!(
        "{}{}",
        charge_item("900001", "700001", "¥ 0", None, "0"),
        charge_item("900002", "700002", "¥ 200", Some("数量: 2"), "30")
    );
    let html = charge_receipt(&items, "¥ 430", "");
    let page = parse(&html, "purchase", "/orders/910001").unwrap();
    let digital: Vec<_> = page
        .rows
        .iter()
        .filter(|row| row.product_id.is_some())
        .collect();
    assert_eq!(
        digital
            .iter()
            .map(|row| row.product_line_total_jpy)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(430)]
    );
    let unsupported = parse(
        &html.replace("¥ 430", "¥ 429"),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    assert_eq!(
        unsupported
            .rows
            .iter()
            .filter(|row| row.product_id.is_some())
            .map(|row| row.product_line_total_jpy)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(430)]
    );
    assert_eq!(
        page.rows
            .iter()
            .map(|row| &row.source_record_id)
            .collect::<Vec<_>>(),
        unsupported
            .rows
            .iter()
            .map(|row| &row.source_record_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(page.fingerprint, unsupported.fingerprint);
    let mut changed = page.rows.clone();
    for row in &mut changed {
        row.product_line_total_jpy = None;
    }
    let proof = product_line_proof(&html, &changed, "910001").unwrap();
    changed[0].source_record_id.push_str("-different");
    apply_product_line_charges(&mut changed, proof, "910001");
    assert_eq!(
        changed
            .iter()
            .map(|row| row.product_line_total_jpy)
            .collect::<Vec<_>>(),
        vec![None, Some(430)]
    );
    let shelf_html = html.replace("<main>", "<main><h1>Library</h1>");
    let shelf = parse(&shelf_html, "library", "/library?page=1").unwrap();
    assert_eq!(
        shelf
            .rows
            .iter()
            .filter(|row| row.product_id.is_some())
            .count(),
        2
    );
    assert!(shelf
        .rows
        .iter()
        .all(|row| row.product_line_total_jpy.is_none()));
}
#[test]
fn old_row_json_defaults_paid_proof_to_none_and_omits_the_new_key() {
    let original = Row {
        source_record_id: "purchase:authored".into(),
        ..Row::default()
    };
    let value = serde_json::to_value(&original).unwrap();
    assert!(value.get("productLineTotalJpy").is_none());
    let restored: Row = serde_json::from_value(value.clone()).unwrap();
    assert!(restored.product_line_total_jpy.is_none());
    assert_eq!(serde_json::to_value(restored).unwrap(), value);
}
#[test]
fn receipt_notice_body_cannot_supply_price_or_acquisition_date() {
    let item = charge_item("900001", "700001", "¥ 100", None, "0");
    let html = charge_receipt(&item, "¥ 100", "");
    for body in [
        "PRIVATE_MESSAGE ¥ -100, quantity 2",
        "注文番号 910001 注文日時 1999/01/01 01:01 お支払金額 ¥ 777777",
    ] {
        let notice = authored_notice(body);
        let with_notice = html.replace("</main>", &format!("{notice}</main>"));
        let parsed = parse(&with_notice, "purchase", "/orders/910001").unwrap();
        assert_bound_product_price(&parsed, 100);
        assert_eq!(
            parsed.rows[0].acquired_date_text.as_deref(),
            Some("2026/10/10 09:30")
        );
        assert!(!serde_json::to_string(&parsed).unwrap().contains("PRIVATE"));
        let without_summary = format!("<main>{item}{notice}</main>");
        let undated = parse(&without_summary, "purchase", "/orders/910001").unwrap();
        assert!(undated
            .rows
            .iter()
            .all(|row| row.acquired_date_text.is_none()));
        assert_bound_product_price(&undated, 100);
    }
}
const TEMPLATE_HTML:&str="<main><div class='sheet' data-product-id='900001'><div><a href='https://invented.booth.pm/items/900001'><img src='https://booth.pximg.net/authored.png'></a><a href='https://invented.booth.pm/items/900001'> Authored \n  product </a><a href='https://invented.booth.pm/'>Authored shop</a><div><span>￥１，５００</span> ｘ <small>１</small></div><b>BOOST↑</b></div><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/700001?variation_id=800001'>Download</a></div></div><div class='sheet'><div><p><b>注文番号</b></p><div># 910001</div></div><div><p><strong>注文日時</strong></p><div>2026/10/10 09:30</div></div></div><div class='sheet'><p>PRIVATE_UNBOUND_SENTINEL</p><div>PRIVATE_ADDRESS_VALUE</div></div></main>";
#[test]
fn receipt_dropdown_proves_owned_variant_without_persisting_extra_url_parameters() {
    let dropdown = serde_json::to_string(&serde_json::json!([
            {"text":"Download","deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?variation_id=800001&client=booth-library-manager","fallbackUrl":"https://booth.pm/","fallbackMessage":""},
            {"text":"Other app","deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?variation_id=800001&client=vroid&type=zip","fallbackUrl":"https://example.invalid/","fallbackMessage":""}
        ])).unwrap();
    let body = format!("<main><div class='sheet sheet--p400'><div class='desktop:flex-1'><div class='text-14'><b><a class='nav' href='https://invented.booth.pm/items/900001'>Authored product (Full Pack)</a></b></div></div><div class='legacy-list-item'><div class='min-w-0'><b>authored.zip</b></div><button data-test='other-downloads-button' data-dropdown-items='{dropdown}'></button></div></div></main>");
    let page = parse(&body, "purchase", "/orders/910001").unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].product_id.as_deref(), Some("900001"));
    assert_eq!(page.rows[0].files.len(), 1);
    assert_eq!(page.rows[0].files[0].id, "700001");
    assert_eq!(page.rows[0].files[0].variant_id.as_deref(), Some("800001"));
    assert_eq!(page.rows[0].files[0].name.as_deref(), Some("authored.zip"));
    assert_eq!(
        page.rows[0].files[0].source_url,
        "https://booth.pm/downloadables/700001?variation_id=800001"
    );
    assert!(page.rows[0].manifest_complete);
}
#[test]
fn receipt_dropdowns_cover_every_file_without_counting_alternate_methods_twice() {
    let mut controls = String::new();
    for offset in 0..7 {
        let file = 700001 + offset;
        let dropdown = serde_json::to_string(&serde_json::json!([
                {"deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id=800001&client=booth-library-manager")},
                {"deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id=800001&client=vroid&type=zip")}
            ])).unwrap();
        controls.push_str(&format!("<div class='legacy-list-item'><div class='min-w-0'><b>authored-{offset}.zip</b></div><div><div><div><div><div><a href='https://booth.pm/downloadables/{file}'>Download</a><button data-test='other-downloads-button' data-dropdown-items='{dropdown}'></button></div></div></div></div></div></div>"));
    }
    let html = format!("<main><div class='sheet'><a href='https://invented.booth.pm/items/900001'>Authored product</a>{controls}</div></main>");
    let page = parse(&html, "purchase", "/orders/910001").unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].files.len(), 7);
    assert!(page.rows[0]
        .files
        .iter()
        .all(|file| file.variant_id.as_deref() == Some("800001")));
    assert!(page.rows[0].files.iter().all(|file| file.name.is_some()
        && !file.source_url.contains("deeplink")
        && !file.source_url.contains("client")));
    assert!(page.rows[0].manifest_complete);
    assert_eq!(page.unresolved, 0);
}
#[test]
fn receipt_control_pairing_is_row_local_and_preserves_distinct_owned_variants() {
    let row = |primary: &str, alternate: &str, variant: &str| {
        let items = serde_json::to_string(&serde_json::json!([
                {"deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{alternate}/deeplink?variation_id={variant}&client=booth-library-manager")}
            ])).unwrap();
        format!("<div class='legacy-list-item'><div class='min-w-0'><b>authored-{variant}.zip</b></div><a href='https://booth.pm/downloadables/{primary}'>Download</a><button data-test='other-downloads-button' data-dropdown-items='{items}'></button></div>")
    };
    let receipt = |rows: String| {
        format!("<main><div class='sheet'><a href='https://invented.booth.pm/items/900001'>Authored product</a>{rows}</div></main>")
    };
    let page = parse(
        &receipt(row("700001", "700001", "800001") + &row("700001", "700001", "800002")),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    assert!(page.rows[0].manifest_complete);
    assert_eq!(page.rows[0].files.len(), 2);
    assert_eq!(
        page.rows[0]
            .files
            .iter()
            .filter_map(|file| file.variant_id.as_deref())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["800001", "800002"])
    );
    let mismatched = parse(
        &receipt(row("700001", "700002", "800001")),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    assert!(!mismatched.rows[0].manifest_complete);
    assert_eq!(mismatched.rows[0].files.len(), 2);
    assert!(mismatched.rows[0]
        .files
        .iter()
        .any(|file| file.id == "700001" && file.variant_id.is_none()));
}
#[test]
fn library_alternates_bind_only_an_observed_primary_within_its_filename_row() {
    let items = serde_json::to_string(&serde_json::json!([
            {"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?variation_id=800001&client=booth-library-manager"},
            {"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?variation_id=800001&client=vroid&type=zip"}
        ])).unwrap();
    for (class, filename) in [
            ("legacy-list-item", "<div class='min-w-0'><b>authored.zip</b></div>"),
            ("file-row", "<div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>authored.zip</div></div>"),
        ] {
            let html = format!("<main><h1>Library</h1><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Authored product</a><div data-test='downloadable-list'><div class='{class}'>{filename}<span data-size-bytes='512'></span><a data-revision='v1' href='https://booth.pm/downloadables/700001'>Download</a><button data-test='other-downloads-button' data-dropdown-items='{items}'></button></div></div></article></main>");
            let page = parse(&html, "library", "/library?page=1").unwrap();
            assert_eq!(page.rows.len(), 1);
            assert_eq!(page.rows[0].files.len(), 1);
            let file = &page.rows[0].files[0];
            assert_eq!(file.id, "700001");
            assert_eq!(file.variant_id.as_deref(), Some("800001"));
            assert_eq!(file.name.as_deref(), Some("authored.zip"));
            assert_eq!(file.size_bytes, Some(512));
            assert_eq!(file.revision, "source:v1");
            assert_eq!(file.source_url, "https://booth.pm/downloadables/700001?variation_id=800001");
            assert!(page.rows[0].manifest_complete);
            assert_eq!(page.unresolved, 0);
        }
}
#[test]
fn library_alternates_cannot_add_files_or_guess_ambiguous_variants() {
    let alternate = |pairs: &[(&str, &str)]| {
        let items: Vec<_> = pairs.iter().map(|(file, variant)| serde_json::json!({
                "deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id={variant}&client=vroid")
            })).collect();
        format!(
            "<button data-test='other-downloads-button' data-dropdown-items='{}'></button>",
            serde_json::to_string(&items).unwrap()
        )
    };
    let primary = "<a data-revision='v1' href='https://booth.pm/downloadables/700001'>Download</a>";
    let file_row = |name: &str, primary: &str, alternate: String| {
        format!("<div class='legacy-list-item'><div class='min-w-0'><b>{name}</b></div>{primary}{alternate}</div>")
    };
    let library = |rows: String| {
        format!("<main><h1>Library</h1><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Authored product</a><div data-test='downloadable-list'>{rows}</div></article></main>")
    };
    for pairs in [
        vec![("700002", "800001")],
        vec![("700001", "800001"), ("700001", "800002")],
    ] {
        let page = parse(
            &library(file_row("authored.zip", primary, alternate(&pairs))),
            "library",
            "/library?page=1",
        )
        .unwrap();
        assert_eq!(page.rows[0].files.len(), 1);
        assert_eq!(page.rows[0].files[0].id, "700001");
        assert_eq!(page.rows[0].files[0].variant_id, None);
    }
    let unobserved = parse(
        &library(file_row(
            "authored.zip",
            "",
            alternate(&[("700001", "800001")]),
        )),
        "library",
        "/library?page=1",
    )
    .unwrap();
    assert!(unobserved.rows[0].files.is_empty());
    assert!(!unobserved.rows[0].manifest_complete);
    let separate = file_row(
        "authored-a.zip",
        primary,
        alternate(&[("700001", "800001")]),
    ) + &file_row(
        "authored-b.zip",
        primary,
        alternate(&[("700001", "800002")]),
    );
    let page = parse(&library(separate), "library", "/library?page=1").unwrap();
    assert_eq!(page.rows[0].files.len(), 2);
    assert_eq!(
        page.rows[0]
            .files
            .iter()
            .filter_map(|file| file.variant_id.as_deref())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["800001", "800002"])
    );
    let known = "<a href='https://booth.pm/downloadables/700001?variation_id=800002'>Download</a>";
    let page = parse(
        &library(file_row(
            "authored.zip",
            known,
            alternate(&[("700001", "800001")]),
        )),
        "library",
        "/library?page=1",
    )
    .unwrap();
    assert_eq!(page.rows[0].files.len(), 1);
    assert_eq!(page.rows[0].files[0].variant_id.as_deref(), Some("800002"));
    let ambiguous_names =
        "<div class='legacy-list-item'><div class='min-w-0'><b>first.zip</b><b>second.zip</b></div>"
            .to_owned()
            + primary
            + &alternate(&[("700001", "800001")]) + "</div>";
    let page = parse(&library(ambiguous_names), "library", "/library?page=1").unwrap();
    assert_eq!(page.rows[0].files[0].variant_id, None);
    for filename in [
            "<div class='min-w-0'><b>authored.zip</b></div>",
            "<div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>authored.zip</div></div>",
        ] {
            let siblings = format!("<div class='legacy-list-item'>{filename}{primary}</div><div class='legacy-list-item'>{}</div>", alternate(&[("700001", "800001")]));
            let page = parse(&library(siblings), "library", "/library?page=1").unwrap();
            assert_eq!(page.rows[0].files.len(), 1);
            assert_eq!(page.rows[0].files[0].variant_id, None);
        }
}
#[test]
fn multitrack_formats_resolve_one_bound_mp3_file_per_track() {
    let menu = |test: &str, items: Vec<serde_json::Value>| {
        format!(
            "<div data-test='{test}' data-dropdown-items='{}'></div>",
            serde_json::to_string(&items).unwrap()
        )
    };
    let track = |formats: &[&str], file: &str, path: &str| {
        let download = menu("track-download-button", formats.iter().map(|kind| serde_json::json!(
                {"text":kind,"path":format!("{path}?type={kind}"),"data":{"filename":format!("track-{file}.{kind}")}})).collect());
        let apps = menu("other-downloads-button", formats.iter().map(|kind| serde_json::json!(
                {"text":"App","deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id=800001&client=booth-library-manager&type={kind}")})).collect());
        download + &apps
    };
    let album = |formats: &[&str], zipped: &[&str]| {
        let zip = menu("zipped-download-button", formats.iter().map(|kind| serde_json::json!(
                {"text":kind,"basename":"authored-album","zippedFiles":zipped.iter().map(|file| serde_json::json!(
                    {"filename":format!("track-{file}.{kind}"),"prefetchUrl":format!("https://booth.pm/downloadables/{file}/archive-part?type={kind}"),"url":""})).collect::<Vec<_>>()})).collect());
        let apps = menu("other-multitrack-downloads-button", formats.iter().map(|kind| serde_json::json!(
                {"text":"App","deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/700001/deeplink?variation_id=800001&client=booth-library-manager&type={kind}&download_scope=album")})).collect());
        zip + &apps
    };
    let pages = |formats: &[&str], paths: [&str; 2], zipped: &[&str]| {
        let (mut library, mut receipt) = (album(formats, zipped), album(formats, zipped));
        for (file, path) in ["700001", "700002"].into_iter().zip(paths) {
            let controls = track(formats, file, path);
            library.push_str(&format!("<div class='mt-16'><div class='flex gap-4 items-center'><div class='break-all text-14 text-text-default'>Track title {file}</div></div><div class='flex'>{controls}</div></div>"));
            receipt.push_str(&format!("<div class='legacy-list-item'><div class='min-w-0'><b>Track title {file}</b></div>{controls}</div>"));
        }
        [
                parse(&format!("<main><h1>Library</h1><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Authored album</a><div>{library}</div></article></main>"), "library", "/library?page=1").unwrap(),
                parse(&format!("<main><div class='sheet'><a href='https://invented.booth.pm/items/900001'>Authored album</a><div class='u-mb-300'>{receipt}</div></div></main>"), "purchase", "/orders/910001").unwrap(),
            ]
    };
    let formats = ["flac", "m4a", "mp3"];
    let tracks = [
        "https://booth.pm/downloadables/700001",
        "https://booth.pm/downloadables/700002",
    ];
    for page in pages(&formats, tracks, &["700001", "700002"]) {
        assert_eq!(page.unresolved, 0);
        assert_eq!(page.rows[0].error_code, None);
        let files: Vec<_> = page.rows[0]
            .files
            .iter()
            .map(|file| (file.id.as_str(), file.variant_id.as_deref()))
            .collect();
        assert_eq!(
            files,
            [("700001", Some("800001")), ("700002", Some("800001"))]
        );
        for file in &page.rows[0].files {
            assert_eq!(file.name, Some(format!("track-{}.mp3", file.id)));
            assert_eq!(file.format.as_deref(), Some("mp3"));
            assert!(file.source_url.ends_with("?variation_id=800001"));
        }
        assert!(page.rows[0].manifest_complete);
    }
    // Without an MP3 choice the same files stay unnamed and uncertified.
    for page in pages(&["flac", "m4a"], tracks, &["700001", "700002"]) {
        assert_eq!(page.rows[0].error_code, None);
        assert_eq!(page.rows[0].files.len(), 2);
        assert!(page.rows[0]
            .files
            .iter()
            .all(|file| file.name.is_none() && file.format.is_none()));
        assert!(!page.rows[0].manifest_complete);
    }
    let outside_zip = pages(&formats, tracks, &["700001", "700009"]);
    let unknown_format = pages(
        &formats,
        [tracks[0], "https://example.invalid/downloadables/700002"],
        &["700001", "700002"],
    );
    for page in outside_zip.into_iter().chain(unknown_format) {
        assert_eq!(
            page.rows[0].error_code.as_deref(),
            Some("DIGITAL_CONTROLS_UNRESOLVED")
        );
    }
}
#[test]
fn purchase_summary_date_enrichment_preserves_native_identity() {
    let before = TEMPLATE_HTML.replace("注文日時", "Unknown label");
    let undated = parse(&before, "purchase", "/orders/910001").unwrap();
    let dated = parse(TEMPLATE_HTML, "purchase", "/orders/910001").unwrap();
    assert_eq!(undated.rows[0].acquired_date_text, None);
    assert_eq!(
        dated.rows[0].acquired_date_text.as_deref(),
        Some("2026/10/10 09:30")
    );
    assert_eq!(
        undated.rows[0].source_record_id,
        dated.rows[0].source_record_id
    );
    assert_eq!(undated.rows[0].product_id, dated.rows[0].product_id);
    assert_eq!(undated.rows[0].order_id, dated.rows[0].order_id);
    assert_eq!(undated.rows[0].files[0].id, dated.rows[0].files[0].id);
    for changed in [
            TEMPLATE_HTML.replace("# 910001", "# 910002"),
            TEMPLATE_HTML.replace("2026/10/10 09:30", "Invalid private value"),
            TEMPLATE_HTML.replace("<p><strong>注文日時</strong></p>", "<p data-product-id='900002'><strong>注文日時</strong></p>"),
            TEMPLATE_HTML.replace("data-product-id='900001'", "data-product-id='900002'"),
            TEMPLATE_HTML.replace("</main>", "<div class='sheet'><div><span>注文番号</span><span>910001</span></div><div><span>注文日時</span><span>2026/10/11</span></div></div></main>"),
        ] {
            let row = parse(&changed, "purchase", "/orders/910001").unwrap().rows.remove(0);
            assert_eq!(row.acquired_date_text, None);
            assert_eq!(row.source_record_id, dated.rows[0].source_record_id);
        }
    // Explicit line identity and duplicate occurrence precedence are unchanged.
    let explicit = TEMPLATE_HTML.replace(
        "class='sheet' data-product-id='900001'",
        "class='sheet' data-product-id='900001' data-line-item-id='920001'",
    );
    let a = parse(
        &explicit.replace("注文日時", "Unknown label"),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    let b = parse(&explicit, "purchase", "/orders/910001").unwrap();
    assert_eq!(a.rows[0].source_record_id, b.rows[0].source_record_id);
    let card = explicit
        .split("<div class='sheet'><div><p><b>注文番号")
        .next()
        .unwrap()
        .strip_prefix("<main>")
        .unwrap();
    let duplicate = explicit.replace("<main>", &format!("<main>{card}"));
    let duplicate_undated = parse(
        &duplicate.replace("注文日時", "Unknown label"),
        "purchase",
        "/orders/910001",
    )
    .unwrap();
    let duplicate_dated = parse(&duplicate, "purchase", "/orders/910001").unwrap();
    assert_eq!(
        duplicate_undated.rows[0].source_record_id,
        duplicate_dated.rows[0].source_record_id
    );
    assert_eq!(
        duplicate_undated.rows[1].source_record_id,
        duplicate_dated.rows[1].source_record_id
    );
    assert!(duplicate_dated.rows[1]
        .source_record_id
        .ends_with(":occurrence:1"));
}
#[test]
fn purchase_legacy_direct_date_retains_original_id_without_proving_acquisition() {
    let legacy = TEMPLATE_HTML.replace(
        "<div data-test='downloadable-list'>",
        "<time datetime='2026/10/10 09:30'></time><div data-test='downloadable-list'>",
    );
    let without_summary = legacy.replace("注文日時", "Unknown label");
    let a = parse(&without_summary, "purchase", "/orders/910001").unwrap();
    let b = parse(&legacy, "purchase", "/orders/910001").unwrap();
    // This is the previous parser's exact fallback preimage, independent of
    // the new summary date. It protects the existing genuinely dated key.
    let old_id = format!(
        "purchase:{}",
        safety::hash("910001:900001::2026/10/10 09:30:")
    );
    assert_eq!(a.rows[0].source_record_id, old_id);
    assert_eq!(b.rows[0].source_record_id, old_id);
    assert_eq!(a.rows[0].acquired_date_text, None);
    assert_eq!(
        b.rows[0].acquired_date_text.as_deref(),
        Some("2026/10/10 09:30")
    );
    let conflicting = legacy.replace("datetime='2026/10/10 09:30'", "datetime='2026/10/11 09:30'");
    let conflict = parse(&conflicting, "purchase", "/orders/910001").unwrap();
    assert_eq!(conflict.rows[0].acquired_date_text, None);
    assert_eq!(
        conflict.rows[0].source_record_id,
        format!(
            "purchase:{}",
            safety::hash("910001:900001::2026/10/11 09:30:")
        )
    );
    let undated = parse(TEMPLATE_HTML, "purchase", "/orders/910001").unwrap();
    assert_eq!(
        undated.rows[0].source_record_id,
        format!("purchase:{}", safety::hash("910001:900001:::"))
    );
    let invalid_manifest = legacy.replace("<b data-test='filename'>authored.zip</b>", "");
    let invalid = parse(&invalid_manifest, "purchase", "/orders/910001").unwrap();
    assert!(!invalid.rows[0].manifest_complete);
    assert_eq!(
        invalid.rows[0].acquired_date_text.as_deref(),
        Some("2026/10/10 09:30")
    );
    assert_eq!(invalid.rows[0].source_record_id, old_id);
    assert_eq!(invalid.rows[0].source_record_id, b.rows[0].source_record_id);
}
const HTML: &str = r#"<main><h1>Library</h1><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Invented avatar</a><div data-test='downloadable-list'><div><b data-test='filename'>source.psd</b><a data-test='downloadable' href='https://booth.pm/downloadables/700001?variation_id=800001'>Download</a><a href='https://booth.pm/downloadables/700001?variation_id=800001'>Again</a></div><div><b data-test='filename'>docs.zip</b><a href='https://booth.pm/downloadables/700002'>Download</a></div></div></article><nav class='pagination'><a aria-current='page' href='/library?page=1'>1</a></nav></main>"#;
#[test]
fn ordering_comes_only_from_what_the_library_page_states_about_its_query() {
    assert_eq!(
        parse(HTML, "library", "/library?page=1").unwrap().ordering,
        Ordering::Unknown
    );
    let stated = |params: &str| {
        let holder = format!("<div class='hidden' data-search-params='{params}' data-search-url='https://accounts.booth.pm/library'></div>");
        HTML.replace("<main>", &format!("<main>{holder}"))
    };
    let read = |html: &str| parse(html, "library", "/library?page=1").unwrap().ordering;
    for (params, expected) in [
        (r#"{"page":1}"#, Ordering::AcquiredNewestFirst),
        (
            r#"{"sort":"library_added_at_desc","page":1}"#,
            Ordering::AcquiredNewestFirst,
        ),
        (
            r#"{"sort":"file_updated_at_desc","page":1}"#,
            Ordering::FileUpdatedNewestFirst,
        ),
        (r#"{"sort":"authored_other","page":1}"#, Ordering::Unknown),
        (r#"{"sort":1,"page":1}"#, Ordering::Unknown),
        (r#"{"q":"authored","page":1}"#, Ordering::Unknown),
        (r#"{"adult":"exclude","page":1}"#, Ordering::Unknown),
        ("not json", Ordering::Unknown),
    ] {
        assert_eq!(read(&stated(params)), expected, "{params}");
    }
    // The words on the page have no say, and neither do other lists.
    let words = "<select><option selected>入手が新しい順</option></select>";
    assert_eq!(
        read(&HTML.replace("<main>", &format!("<main>{words}"))),
        Ordering::Unknown
    );
    for (source, route) in [
        ("gift", "/library/gifts?page=1"),
        ("free", "/library/free_downloads?page=1"),
    ] {
        let html = stated(r#"{"page":1}"#).replace("/library?page=1", route);
        assert_eq!(
            parse(&html, source, route).unwrap().ordering,
            Ordering::Unknown,
            "{source}"
        );
    }
}
#[test]
fn a_sorted_library_page_goes_on_in_its_own_order() {
    let sorted = |current: &str, more: &str| {
        format!("<html><head><meta name='js_const_user_signed_in' content='true'></head><body><main role='main'><div class='hidden' data-search-params='{{\"sort\":\"file_updated_at_desc\",\"page\":{current}}}'></div><nav><a class='nav-item current' href='https://accounts.booth.pm/library?page=9'>Authored tab</a></nav><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Authored</a><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/700001'>Download</a></div></article><div class='pager border-0'><nav><ul>{more}</ul></nav></div></main></body></html>")
    };
    let first = sorted("1", "<li class='current'><a class='nav-item' href='/library?sort=file_updated_at_desc'>1</a></li><li><a class='nav-item' href='/library?page=2&amp;sort=file_updated_at_desc'>2</a></li><li><a class='nav-item' rel='next' href='/library?page=2&amp;sort=file_updated_at_desc'>n</a></li>");
    let page = parse(&first, "library", &safety::updated_route(1)).unwrap();
    assert_eq!(page.route, "/library?sort=file_updated_at_desc&page=1");
    assert_eq!(page.next, [safety::updated_route(2)]);
    assert_eq!(page.ordering, Ordering::FileUpdatedNewestFirst);
    assert!(!page.terminal);
    let last = sorted("2", "<li><a class='nav-item' rel='prev' href='/library?sort=file_updated_at_desc'>p</a></li><li class='current'><a class='nav-item' href='/library?page=2&amp;sort=file_updated_at_desc'>2</a></li>");
    let page = parse(&last, "library", &safety::updated_route(2)).unwrap();
    assert!(page.next.is_empty());
    assert!(page.terminal);
}
#[test]
fn scoped_names_manifest_and_identity() {
    let p = parse(HTML, "library", "/library?page=1").unwrap();
    assert!(p.terminal);
    assert_eq!(p.rows[0].files.len(), 2);
    assert_eq!(p.rows[0].files[0].name.as_deref(), Some("source.psd"));
    assert!(p.rows[0].manifest_complete);
    assert_eq!(p.rows[0].files[0].revision, "unknown");
}
#[test]
fn no_inferred_terminal_or_manifest() {
    let p = parse(
        &HTML
            .replace("data-test='downloadable-list'", "")
            .replace("aria-current='page'", ""),
        "library",
        "/library?page=1",
    )
    .unwrap();
    assert!(!p.terminal);
    assert!(!p.rows[0].manifest_complete);
}
#[test]
fn ambiguous_names_are_unknown() {
    let p = parse(
        &HTML.replace(
            "<b data-test='filename'>source.psd</b>",
            "<b data-test='filename'>first.psd</b><b data-test='filename'>second.psd</b>",
        ),
        "library",
        "/library?page=1",
    )
    .unwrap();
    assert!(p.rows[0].files[0].name.is_none());
    assert!(!p.rows[0].manifest_complete);
}
#[test]
fn orders_do_not_project_nondigital_content() {
    let h = "<main><div class='sheet'><p>PRIVATE ADDRESS MUST NOT PROJECT</p></div></main>";
    let p = parse(h, "purchase", "/orders/900001").unwrap();
    assert!(p.rows.is_empty());
    assert_eq!(p.unresolved, 0);
    assert!(!serde_json::to_string(&p).unwrap().contains("PRIVATE"));
}
#[test]
fn source_navigation_cannot_finish_other_source() {
    let h = HTML.replace("/library?page=1", "/orders?page=1");
    assert!(!parse(&h, "library", "/library?page=1").unwrap().terminal);
}
#[test]
fn wrapped_order_sheet_resolves_only_its_own_bounded_anchor() {
    let h="<main><h1>Orders</h1><a href='/orders/900001'><div class='sheet'><span>Independently authored order</span></div></a><nav class='pagination'><a aria-current='page' href='/orders'>1</a></nav></main>";
    let page = parse(h, "purchase", "/orders?page=1").unwrap();
    assert_eq!(page.details, vec!["/orders/900001"]);
    assert_eq!(page.unresolved, 0);
    assert!(page.terminal);
    let unrelated="<main><h1>Orders</h1><a href='/orders/900001'><div><div class='sheet'>One</div><div class='sheet'>Two</div></div></a></main>";
    let page = parse(unrelated, "purchase", "/orders?page=1").unwrap();
    assert!(page.details.is_empty());
    assert_eq!(page.unresolved, 2);
    let spoof = h.replace(
        "/orders/900001",
        "https://accounts.booth.pm.evil.test/orders/900001",
    );
    assert!(parse(&spoof, "purchase", "/orders?page=1")
        .unwrap()
        .details
        .is_empty());
}
#[test]
fn canonical_page_one_pager_does_not_turn_navigation_or_spans_into_terminal() {
    let canonical = HTML.replace("href='/library?page=1'", "href='/library'");
    assert!(
        parse(&canonical, "library", "/library?page=1")
            .unwrap()
            .terminal
    );
    assert!(
        !parse(&canonical, "library", "/library?page=2")
            .unwrap()
            .terminal
    );
    let next = canonical.replace(
        "</nav>",
        "<a rel='next' href='/library?page=2'>Next</a></nav>",
    );
    let page = parse(&next, "library", "/library?page=1").unwrap();
    assert!(!page.terminal);
    assert_eq!(page.next, vec!["/library?page=2"]);
    let prev = HTML
        .replace("href='/library?page=1'", "href='/library?page=2'")
        .replace("</nav>", "<a rel='prev' href='/library'>Previous</a></nav>");
    let page = parse(&prev, "library", "/library?page=2").unwrap();
    assert!(page.terminal);
    assert!(page.next.is_empty());
    let navigation = canonical.replace("<nav class='pagination'>", "<nav>");
    assert!(
        !parse(&navigation, "library", "/library?page=1")
            .unwrap()
            .terminal
    );
    let span = canonical.replace(
        "<a aria-current='page' href='/library'>1</a>",
        "<span aria-current='page'>1</span><a rel='prev' href='/library'>Previous</a>",
    );
    assert!(!parse(&span, "library", "/library?page=1").unwrap().terminal);
}
#[test]
fn signed_control_projects_only_stable_ids() {
    let h = HTML.replace(
        "?variation_id=800001",
        "?variation_id=800001&amp;signature=PRIVATE_SIGNING_VALUE",
    );
    let p = parse(&h, "library", "/library?page=1").unwrap();
    assert_eq!(p.rows[0].files[0].variant_id.as_deref(), Some("800001"));
    assert!(!serde_json::to_string(&p)
        .unwrap()
        .contains("PRIVATE_SIGNING"));
}
#[test]
fn added_files_do_not_change_acquisition_identity() {
    let h = HTML.replace("700002", "700099");
    let a = parse(HTML, "library", "/library?page=1").unwrap();
    let b = parse(&h, "library", "/library?page=1").unwrap();
    assert_eq!(a.rows[0].source_record_id, b.rows[0].source_record_id);
    assert_ne!(a.fingerprint, b.fingerprint);
}
/// The shapes real account pages use, with every word replaced by a language's wording.
fn library_page(current: &str, body: &str) -> String {
    let tab = |path: &str| {
        let class = if path == current {
            "nav-item current"
        } else {
            "nav-item"
        };
        format!("<li><a class='{class}' href='https://accounts.booth.pm{path}?page=1'>Authored tab</a></li>")
    };
    format!("<html lang='zh-TW'><head><meta name='js_const_user_signed_in' content='true'></head><body><main role='main'><div class='text-center'><h1 class='typography-16'>Authored heading</h1></div><nav class='ui-segmented-tablet-nav centered'><ul>{}{}{}</ul></nav><div class='flex gap-40'><div class='w-full flex flex-col gap-16'><div class='h-32'></div>{body}</div></div></main></body></html>",
            tab("/library"), tab("/library/gifts"), tab("/library/free_downloads"))
}
#[test]
fn empty_lists_are_told_by_layout_in_every_language() {
    for sentence in [
        "受け取ったギフトはありません",
        "No gifts received",
        "받은 선물이 없습니다",
        "没有收到的礼物",
        "沒有收到的禮物",
    ] {
        let panel = format!("<div class='bg-white text-center text-16 font-bold text-text-gray600 py-64 desktop:rounded-8'>{sentence}</div>");
        for (source, path) in [
            ("library", "/library"),
            ("gift", "/library/gifts"),
            ("free", "/library/free_downloads"),
        ] {
            let page = parse(
                &library_page(path, &panel),
                source,
                &format!("{path}?page=1"),
            )
            .unwrap();
            assert!(
                page.terminal && page.rows.is_empty() && page.unresolved == 0,
                "{source}"
            );
            assert!(page.next.is_empty());
        }
        let orders = format!("<html><head><meta name='js_const_user_signed_in' content='true'></head><body><main role='main'><div class='manage-page-head'><h1 class='manage-page-head-title'>Authored heading</h1><div><a href='https://accounts.booth.pm/library'><span>Authored link</span></a></div></div><div class='manage-page-body'><div class='container border-b'></div><div class='container static-3of4 l-orders-index'><div class='u-mt-500 text-center'>{sentence}</div></div></div></main></body></html>");
        let page = parse(&orders, "purchase", "/orders?page=1").unwrap();
        assert!(page.terminal && page.details.is_empty() && page.unresolved == 0);
    }
    // A list that shrank while it was read: BOOTH's pager goes on past the end, the App does
    // not, and a row may have moved past what was read, so it does not certify the list.
    let past_end = library_page("/library/gifts", "<div class='bg-white text-center py-64'>Authored sentence</div><div class='pager border-0'><nav><ul><li><a class='nav-item' rel='prev' href='https://accounts.booth.pm/library/gifts?page=8'>p</a></li><li><a class='nav-item' rel='next' href='https://accounts.booth.pm/library/gifts?page=10'>n</a></li></ul></nav></div>");
    let page = parse(&past_end, "gift", "/library/gifts?page=9").unwrap();
    assert!(page.terminal && page.next.is_empty());
    assert_eq!(page.unresolved, 1);
    // Only that panel says so: a page without it, or holding more than a sentence, stays unread.
    for body in [
            String::new(),
            "<div class='bg-white text-center'></div>".to_owned(),
            "<div class='text-center'>Authored sentence</div>".to_owned(),
            "<div class='bg-white text-center'><a href='/authored'>Authored link</a></div>".to_owned(),
            "<div class='bg-white text-center'>Authored sentence</div><div class='bg-white text-center'>Authored sentence</div>".to_owned(),
        ] {
            assert_eq!(
                parse(&library_page("/library/gifts", &body), "gift", "/library/gifts?page=1").unwrap_err().to_string(),
                "NO_RECOGNIZED_ROWS",
                "{body}"
            );
        }
    // Another page's layout does not stand in for the order list.
    assert_eq!(
            parse("<main><h1>Authored heading</h1><div class='bg-white text-center'>Authored sentence</div></main>", "purchase", "/orders?page=1").unwrap_err().to_string(),
            "SOURCE_MARKER_UNKNOWN"
        );
    assert_eq!(
            parse("<main><div class='l-orders-index'><div class='bg-white'>Authored sentence</div></div></main>", "purchase", "/orders?page=1").unwrap_err().to_string(),
            "NO_RECOGNIZED_ROWS"
        );
    assert_eq!(
            parse("<main><div class='l-orders-index'><div class='text-center'>Authored sentence</div><div class='text-center'>Authored sentence</div></div></main>", "purchase", "/orders?page=1").unwrap_err().to_string(),
            "NO_RECOGNIZED_ROWS"
        );
}
#[test]
fn a_list_of_one_page_ends_at_its_empty_pager() {
    let row = "<div class='bg-white p-16 desktop:rounded-8'><b><a class='nav' href='https://authored.booth.pm/items/900001'>Authored title</a></b><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/700001'>Download</a></div></div>";
    let html = |pager: &str| library_page("/library/gifts", &format!("{row}{pager}"));
    let page = parse(
        &html("<div class='pager border-0'></div>"),
        "gift",
        "/library/gifts?page=1",
    )
    .unwrap();
    assert!(page.terminal && page.next.is_empty() && page.unresolved == 0);
    assert_eq!(page.rows[0].product_id.as_deref(), Some("900001"));
    let orders = "<main role='main'><div class='container l-orders-index'><a href='https://accounts.booth.pm/orders/910001'><div class='sheet'>Authored order</div></a><div class='pager'></div></div></main>";
    let page = parse(orders, "purchase", "/orders?page=1").unwrap();
    assert!(page.terminal && page.next.is_empty());
    assert_eq!(page.details, ["/orders/910001"]);
    // Anything else stays unproven: no pager at all, one holding something, or a later page.
    for pager in [
        "",
        "<div class='pager border-0'><nav></nav></div>",
        "<div class='other-pager'></div>",
    ] {
        let page = parse(&html(pager), "gift", "/library/gifts?page=1").unwrap();
        assert!(!page.terminal, "{pager}");
    }
    let page = parse(
        &html("<div class='pager border-0'></div>"),
        "gift",
        "/library/gifts?page=2",
    )
    .unwrap();
    assert!(!page.terminal);
}
#[test]
fn a_delisted_row_is_told_by_its_faded_thumbnail_in_every_language() {
    let row = |image: &str, badge: &str, title: &str| {
        format!("<div class='bg-white p-16 desktop:rounded-8'><div class='flex gap-8'><img class='{image}' src='https://booth.pximg.net/authored.png'><div><div class='bg-primary700 rounded-4 px-8 w-fit text-white text-12 font-bold'>{badge}</div>{title}<a class='no-underline w-fit flex' rel='noopener' href='https://authored.booth.pm/'><img class='rounded-[50%] w-24 h-24'><div class='text-14'>Authored shop</div></a></div></div></div>")
    };
    let plain = "<div class='text-text-default font-bold text-16 py-4 break-all opacity-[0.32]'>Authored title</div>";
    let html = |card: String| {
        library_page(
                "/library/free_downloads",
                &format!("{card}<div class='pager border-0'><nav><ul><li class='current'><a class='nav-item' href='https://accounts.booth.pm/library/free_downloads?page=1'>1</a></li></ul></nav></div>"),
            )
    };
    let route = "/library/free_downloads?page=1";
    for badge in ["非公開", "Private", "비공개", "未公开", "未公開"] {
        let page = parse(
            &html(row("l-library-item-thumbnail opacity-[0.32]", badge, plain)),
            "free",
            route,
        )
        .unwrap();
        assert_eq!(page.unresolved, 0, "{badge}");
        assert!(page.terminal);
        assert!(page.rows[0].unavailable && page.rows[0].product_id.is_none());
        assert_eq!(
            page.rows[0].error_code.as_deref(),
            Some("BOOTH_WITHHELD_PRODUCT_ID")
        );
    }
    // A row that merely lost its link, unfaded and with unknown wording, is still unread.
    let page = parse(
        &html(row("l-library-item-thumbnail", "Private", plain)),
        "free",
        route,
    )
    .unwrap();
    assert_eq!(page.unresolved, 1);
    assert_eq!(
        page.rows[0].error_code.as_deref(),
        Some("PRODUCT_ID_UNRESOLVED")
    );
    assert!(!page.rows[0].unavailable);
    // A faded row that still links its product keeps that identity.
    let linked =
        "<b><a class='nav' href='https://authored.booth.pm/items/900001'>Authored title</a></b>";
    let page = parse(
        &html(row(
            "l-library-item-thumbnail opacity-[0.32]",
            "Private",
            linked,
        )),
        "free",
        route,
    )
    .unwrap();
    assert_eq!(page.rows[0].product_id.as_deref(), Some("900001"));
    assert!(page.rows[0].unavailable);
    assert_eq!(page.unresolved, 0);
}
#[test]
fn exact_withheld_marker_keeps_unknown_id_but_not_unsupported_identity() {
    let h="<h1>Library</h1><div class='bg-white p-16'><div>この商品は非公開です</div></div><nav class='pagination'><a aria-current='page' href='/library?page=1'>1</a></nav>";
    let p = parse(h, "library", "/library?page=1").unwrap();
    assert_eq!(p.unresolved, 0);
    assert!(p.rows[0].unavailable);
    assert!(p.rows[0].product_id.is_none());
    assert_eq!(
        p.rows[0].error_code.as_deref(),
        Some("BOOTH_WITHHELD_PRODUCT_ID")
    );
    let p = parse(
        &h.replace("<div>この商品", "<div data-item-id='900001'>この商品"),
        "library",
        "/library?page=1",
    )
    .unwrap();
    assert_eq!(p.unresolved, 1);
    assert_eq!(
        p.rows[0].error_code.as_deref(),
        Some("PRODUCT_ID_UNRESOLVED")
    );
}
const MANIFEST_HTML: &str = r#"<main><h1>Library</h1><article data-test='library-item'><a href='https://synthetic.booth.pm/items/900001'>PRIVATE_TITLE</a><div data-test='downloadable-list'><div><b data-test='filename'>PRIVATE_FILE.psd</b><a data-test='downloadable' href='https://booth.pm/downloadables/700001?variation_id=800001'>Download</a></div></div></article><nav class='pagination'><a aria-current='page' href='/library?page=1'>1</a></nav></main>"#;
fn modern_structure() -> String {
    MANIFEST_HTML.replace("data-test='downloadable-list'", "class='synthetic-file-container'").replace("<b data-test='filename'>PRIVATE_FILE.psd</b>", "<div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_FILE.psd</div></div>")
}
#[test]
fn each_manifest_gate_leaves_a_library_row_incomplete() {
    assert!(
        parse(MANIFEST_HTML, "library", "/library?page=1")
            .unwrap()
            .rows[0]
            .manifest_complete
    );
    for (html, gate) in [
        (
            MANIFEST_HTML.replace("data-test='downloadable-list'", ""),
            "rowsMissingExplicitList",
        ),
        (
            MANIFEST_HTML.replace("<b data-test='filename'>PRIVATE_FILE.psd</b>", ""),
            "rowsMissingFilename",
        ),
        (
            MANIFEST_HTML.replace(
                "<a data-test='downloadable'",
                "<i data-test='downloadable'></i><a data-test='downloadable'",
            ),
            "rowsInvalidControls",
        ),
        (
            MANIFEST_HTML.replace(
                "<article data-test='library-item'>",
                "<article data-test='library-item'><i data-test='item-unavailable'></i>",
            ),
            "rowsUnavailable",
        ),
        (
            MANIFEST_HTML.replace(
                "https://synthetic.booth.pm/items/900001",
                "/unknown-synthetic-route",
            ),
            "rowsWithError",
        ),
        (
            MANIFEST_HTML.replace(
                "https://booth.pm/downloadables/700001?variation_id=800001",
                "/unknown-synthetic-download",
            ),
            "rowsWithoutFiles",
        ),
    ] {
        let page = parse(&html, "library", "/library?page=1").unwrap();
        assert!(!page.rows[0].manifest_complete, "{gate}");
    }
}
#[test]
fn modern_shelf_manifest_preserves_all_other_gates_and_rejects_partial_structures() {
    let html = modern_structure();
    let complete = |body: &str, source: &str, route: &str| {
        parse(body, source, route).unwrap().rows[0].manifest_complete
    };
    assert!(complete(&html, "library", "/library?page=1"));
    for (source, route) in [
        ("gift", "/library/gifts?page=1"),
        ("free", "/library/free_downloads?page=1"),
    ] {
        assert!(complete(
            &html.replace("<h1>Library</h1>", "<h1>Gift Free downloads</h1>"),
            source,
            route
        ));
    }
    // Purchase has not established this modern shelf witness.
    assert!(!complete(
        &html.replace("data-test='library-item'", "class='sheet'"),
        "purchase",
        "/orders/900009"
    ));
    let alternate=html.replace("</article>","<button data-test='other-downloads-button' aria-expanded='false' data-dropdown-items='PRIVATE_METHODS'>PRIVATE_METHOD</button></article>");
    assert!(complete(&alternate, "library", "/library?page=1"));
    let signed = html.replace(
        "variation_id=800001",
        "variation_id=800001&amp;signature=PRIVATE_TOKEN",
    );
    let page = parse(&signed, "library", "/library?page=1").unwrap();
    assert!(page.rows[0].manifest_complete);
    let duplicate_action = html.replace("</div></div></article>", "<a href='https://booth.pm/downloadables/700001?variation_id=800001'>PRIVATE_AGAIN</a></div></div></article>");
    assert!(complete(&duplicate_action, "library", "/library?page=1"));
    assert!(!page.rows[0].files[0].source_url.contains("PRIVATE_TOKEN"));
    let multiple=html.replace("</div></div></article>","</div><div><div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_DOC.zip</div></div><a data-test='downloadable' href='https://booth.pm/downloadables/700002'>Download</a></div></div></article>");
    let page = parse(&multiple, "library", "/library?page=1").unwrap();
    assert_eq!(page.rows[0].files.len(), 2);
    assert!(page.rows[0].manifest_complete);
    let orphan = html.replace("</div></div></article>", "</div><div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_ORPHAN.zip</div></div></div></article>");
    let conflicting = html.replace("</div></div></article>", "</div><div><div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_CONFLICT.zip</div></div><a data-test='downloadable' href='https://booth.pm/downloadables/700001?variation_id=800001'>Download</a></div></div></article>");
    let nested_orphan=html.replace("<div class='synthetic-file-container'>","<div class='synthetic-file-container'><div class='synthetic-inner'>").replace("</div></div></article>","</div></div><div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_NESTED_ORPHAN.zip</div></div></div></article>");
    let linked_outer_orphan = html.replace("<div class='synthetic-file-container'>", "<div class='synthetic-outer'><a href='https://synthetic.booth.pm/items/900001'>PRIVATE_SAME_PRODUCT</a><div class='synthetic-file-container'>").replace("</div></div></article>", "</div></div><div class='min-w-0 break-words whitespace-pre-line'><div class='text-14'>PRIVATE_OUTSIDE_ORPHAN.zip</div></div></div></article>");
    let linked_page = parse(&linked_outer_orphan, "library", "/library?page=1").unwrap();
    assert_eq!(linked_page.unresolved, 0);
    assert!(linked_page.rows[0].error_code.is_none());
    assert!(!linked_page.rows[0].manifest_complete);
    for body in [&orphan, &conflicting, &nested_orphan] {
        assert!(!complete(body, "library", "/library?page=1"));
    }
    let missing_list = MANIFEST_HTML.replace("data-test='downloadable-list'", "");
    assert!(!complete(&missing_list, "library", "/library?page=1"));
    for (body, reason) in [
            (html.replace("</article>","<button data-test='show-more'>PRIVATE_MORE</button></article>"),"load more"),
            (html.replace("</article>","<button aria-expanded='false'>PRIVATE_EXPAND</button></article>"),"collapsed control"),
            (html.replace("</article>","<select name='variation_id'></select></article>"),"variant selector"),
            (html.replace("</article>","<button data-dropdown-items='PRIVATE_UNKNOWN'></button></article>"),"unknown dropdown"),
            (html.replace("<article data-test='library-item'>","<article data-test='library-item' data-file-count='2'>"),"advertised mismatch"),
            (html.replace("<article data-test='library-item'>","<article data-test='library-item' data-file-count='PRIVATE_COUNT'>"),"advertised invalid"),
            (html.replace("PRIVATE_FILE.psd</div>","PRIVATE_FILE.psd</div><div class='text-14'>PRIVATE_OTHER.zip</div>"),"ambiguous name"),
            (html.replace("<div class='synthetic-file-container'>","").replace("</div></div></article>","</div></article>"),"broad card only"),
            (html.replace("<div class='synthetic-file-container'>","<div class='synthetic-file-container'><a href='https://synthetic.booth.pm/items/900002'>PRIVATE_SECOND</a>"),"multiple products"),
            (html.replace("<a data-test='downloadable'","<i data-test='downloadable'></i><a data-test='downloadable'"),"invalid resolver"),
            (html.replace("variation_id=800001","variation_id=PRIVATE_INVALID"),"invalid variant"),
            (html.replace("<article data-test='library-item'>","<article data-test='library-item'><i data-test='item-unavailable'></i>"),"unavailable"),
        ] { assert!(!complete(&body,"library","/library?page=1"),"{reason}"); }
    assert!(complete(
        &html.replace(
            "<article data-test='library-item'>",
            "<article data-test='library-item' data-file-count='1'>"
        ),
        "library",
        "/library?page=1"
    ));
}
