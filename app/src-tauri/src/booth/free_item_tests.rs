use super::*;
// Independently authored structural fixture, no provider page or script copied.
const HTML: &str = r#"<html><head><link rel='canonical' href='https://booth.pm/ja/items/900001'><meta property='og:url' content='https://booth.pm/ja/items/900001'></head><body><div id='items' class='market' data-product-id='900001'><ul id='variations'><li class='variation-item'><div class='cart-button-wrap'><a title='authored.zip' href='https://booth.pm/downloadables/700001?variation_id=800001'><span class='cmd-label'>無料ダウンロード</span></a><div class='text-14 text-text-default font-normal text-left mb-8'>authored.zip</div></div></li><li class='variation-item'><form><button class='add-cart' data-product-id='900001'>Authored donation</button></form></li></ul></div></body></html>"#;
#[test]
fn bound_roles_and_static_markers_never_certify_unavailability() {
    let gift = "<li class='variation-item'><form><button class='add-gift' data-product-id='900001' disabled>PRIVATE_ROLE_SENTINEL</button></form></li>";
    let unknown = "<li class='variation-item'><form><button class='authored-unknown' aria-disabled='true'>PRIVATE_ROLE_SENTINEL</button></form><span class='item-unavailable'>PRIVATE_ROLE_SENTINEL</span></li>";
    let html = HTML.replace("</ul>", &format!("{gift}{unknown}</ul>"));
    let evidence = parse(&html, "900001").unwrap();
    assert_eq!(
        evidence.error_code.as_deref(),
        Some("FREE_ITEM_ROLE_UNPROVEN")
    );
    assert!(!evidence.manifest_complete);
    assert!(!evidence.unavailable);

    let start = HTML.find("<li class='variation-item'>").unwrap();
    let end = start + HTML[start..].find("</li>").unwrap() + "</li>".len();
    let paid_only = format!("{}{}", &HTML[..start], &HTML[end..]);
    let evidence = parse(&paid_only, "900001").unwrap();
    assert_eq!(
        evidence.error_code.as_deref(),
        Some("FREE_FILE_CONTROLS_UNAVAILABLE")
    );
    assert!(!evidence.manifest_complete);
    assert!(!evidence.unavailable);

    let closed_rows = "<li class='variation-item'><div class='variation-price'>¥ 500</div><button class='add-cart disabled' type='button'>PRIVATE_ROLE_SENTINEL</button></li><li class='variation-item'><div class='variation-price'>¥ 700</div><div class='restock-request-links'><a class='js-add-restock-request'>PRIVATE_ROLE_SENTINEL</a><a class='js-remove-restock-request'>PRIVATE_ROLE_SENTINEL</a></div></li>";
    let closed = HTML.replace("</ul>", &format!("{closed_rows}</ul>"));
    let evidence = parse(&closed, "900001").unwrap();
    assert!(evidence.error_code.is_none());
    assert!(evidence.manifest_complete);
    assert_eq!(evidence.files.len(), 1);
    assert!(!evidence.unavailable);
    // BOOTH's other languages write the same closed rows' prices in JPY.
    let elsewhere = closed
        .replace("¥ 500", "500 JPY")
        .replace("¥ 700", "700 JPY");
    let evidence = parse(&elsewhere, "900001").unwrap();
    assert!(evidence.error_code.is_none() && evidence.manifest_complete);
    let free = elsewhere.replace("500 JPY", "0 JPY");
    assert_eq!(
        parse(&free, "900001").unwrap().error_code.as_deref(),
        Some("FREE_ITEM_ROLE_UNPROVEN")
    );
}
#[test]
fn production_accepts_any_same_item_url() {
    let original = "https://booth.pm/ja/items/900001";
    // Production needs this item's ID on an HTTPS BOOTH page, not one URL spelling.
    for (url, same_item) in [
        (original, true),
        ("https://booth.pm/items/900001", true),
        ("https://booth.pm/en/items/900001", true),
        ("https://synthetic-shop.booth.pm/items/900001", true),
        ("https://synthetic-shop.booth.pm/ja/items/900001", true),
        ("https://synthetic-shop.booth.pm/en/items/900001", true),
        ("https://booth.pm/ja/items/900002", false),
        ("https://www.booth.pm/items/900001", true),
        ("https://accounts.booth.pm/items/900001", false),
        ("https://synthetic-shop.booth.pm/items/900001?x=1", true),
        ("https://synthetic-shop.booth.pm:443/items/900001", true),
        ("https://synthetic-shop.booth.pm/ja/../items/900001", true),
        ("https://synthetic-shop.booth.pm/items/900001/", false),
        ("https://synthetic-shop.booth.pm/items/900002", false),
        (
            "https://synthetic-shop.booth.pm.evil.test/items/900001",
            false,
        ),
        ("http://synthetic-shop.booth.pm/items/900001", false),
    ] {
        let html = HTML.replace(original, url);
        let evidence = parse(&html, "900001").unwrap();
        assert_eq!(evidence.manifest_complete, same_item, "{url}");
        if !same_item {
            assert_eq!(
                evidence.error_code.as_deref(),
                Some("FREE_ITEM_IDENTITY_UNPROVEN")
            );
        }
    }
}
#[test]
fn proven_shared_manifest_with_nested_entries_is_complete() {
    let original = HTML
        .split_once("<li class='variation-item'>")
        .unwrap()
        .1
        .split_once("</li>")
        .unwrap()
        .0;
    let first = original
        .strip_prefix("<div class='cart-button-wrap'>")
        .unwrap()
        .strip_suffix("</div>")
        .unwrap();
    let second = first
        .replace("700001", "700002")
        .replace("800001", "800002")
        .replace("authored.zip", "second.psd");
    let multi =
        format!("<div class='cart-button-wrap'><div>{first}</div><div>{second}</div></div>");
    let html = HTML.replacen(original, &multi, 1);
    assert!(parse(&html, "900001").unwrap().manifest_complete);
}
fn shared_wrap_html() -> String {
    shared_wrap_html_with_files(4)
}
fn shared_wrap_html_with_files(count: usize) -> String {
    // Reordered independent filename nodes and same-file menus in one wrap.
    let controls = (1..=count).map(|i| format!("<a title='authored-{i}.zip' href='https://booth.pm/downloadables/70000{i}?variation_id=80000{i}'><span class='cmd-label'>無料ダウンロード</span></a>")).collect::<String>();
    let names = (1..=count).rev().map(|i| format!("<div class='text-14 text-text-default font-normal text-left mb-8'>authored-{i}.zip</div>")).collect::<String>();
    let menus = (1..=count).map(|i| format!(r#"<div class='js-download-free-button' data-test='other-downloads-button' data-dropdown-items='[{{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/70000{i}/deeplink?client=booth-library-manager&variation_id=80000{i}"}}]'></div>"#)).collect::<String>();
    let original = HTML
        .split_once("<li class='variation-item'>")
        .unwrap()
        .1
        .split_once("</li>")
        .unwrap()
        .0;
    HTML.replacen(
        original,
        &format!("<div class='cart-button-wrap'>{controls}{names}{menus}</div>"),
        1,
    )
}
#[test]
fn a_refused_free_file_reads_as_ended_only_when_its_page_says_so() {
    use super::super::free_offer_closed;
    let start = HTML.find("<li class='variation-item'>").unwrap();
    let end = start + HTML[start..].find("</li>").unwrap() + "</li>".len();
    let paid = format!("{}{}", &HTML[..start], &HTML[end..]).replace(
        "<form><button",
        "<div class='variation-price'>¥ 1,200</div><form><button",
    );
    assert!(free_offer_closed(200, &paid, "900001"));
    assert!(free_offer_closed(404, "", "900001"));
    assert!(free_offer_closed(410, "", "900001"));
    // Still offered, unreadable, or a status that says nothing about the offer.
    assert!(!free_offer_closed(200, HTML, "900001"));
    assert!(!free_offer_closed(200, "<html></html>", "900001"));
    assert!(!free_offer_closed(200, &paid, "900002"));
    assert!(!free_offer_closed(403, &paid, "900001"));
    assert!(!free_offer_closed(500, &paid, "900001"));
}
#[test]
fn a_refused_free_file_reads_as_removed_when_its_open_offer_no_longer_lists_it() {
    use super::super::free_refusal;
    let file = |id: &str, variant: &str| serde_json::json!({"productId":"900001","sourceFileId":id,"variantId":variant});
    // The offer goes on with another file in the same variation, as after a new version.
    assert_eq!(
        free_refusal(200, HTML, "900001", &file("700009", "800001")),
        Some("FILE_UNAVAILABLE")
    );
    // Still listed, another variation, or a page that proves nothing: the refusal stands.
    assert_eq!(
        free_refusal(200, HTML, "900001", &file("700001", "800001")),
        None
    );
    assert_eq!(
        free_refusal(200, HTML, "900001", &file("700009", "800002")),
        None
    );
    assert_eq!(
        free_refusal(200, "<html></html>", "900001", &file("700009", "800001")),
        None
    );
    assert_eq!(
        free_refusal(500, HTML, "900001", &file("700009", "800001")),
        None
    );
    assert_eq!(
        free_refusal(404, "", "900001", &file("700009", "800001")),
        Some("FREE_OFFER_UNAVAILABLE")
    );
}
#[test]
fn settled_free_offer_absence_requires_positive_paid_rows_and_all_guards() {
    let start = HTML.find("<li class='variation-item'>").unwrap();
    let end = start + HTML[start..].find("</li>").unwrap() + "</li>".len();
    let paid = format!("{}{}", &HTML[..start], &HTML[end..]).replace(
        "<form><button",
        "<div class='variation-price'>¥ 1,200</div><form><button",
    );
    for html in [
            paid.clone(),
            paid.replace("class='add-cart'", "class='add-gift'"),
            paid.replace("<form><button class='add-cart' data-product-id='900001'>Authored donation</button></form>", "<button class='add-cart disabled' type='button'>Authored paused</button>"),
            paid.replace("<form><button class='add-cart' data-product-id='900001'>Authored donation</button></form>", "<div class='restock-request-links'><a class='js-add-restock-request'>Authored notification</a></div>"),
        ] {
            let evidence = parse(&html, "900001").unwrap();
            assert_eq!(evidence.error_code.as_deref(), Some("FREE_OFFER_UNAVAILABLE"));
            assert!(evidence.unavailable);
            assert!(!evidence.manifest_complete);
            assert!(evidence.files.is_empty());
            assert_eq!(evidence.page().unwrap().unresolved, 1);
        }
    for price in ["¥ 0", "¥ 1,200–2,000", "$ 1,200", "1200", "¥ 12,00"] {
        let evidence = parse(&paid.replace("¥ 1,200", price), "900001").unwrap();
        assert_eq!(
            evidence.error_code.as_deref(),
            Some("FREE_FILE_CONTROLS_UNAVAILABLE")
        );
        assert!(!evidence.unavailable);
        assert!(!evidence.manifest_complete);
    }
    for (html, code) in [
            (paid.replace("<div class='variation-price'>¥ 1,200</div>", ""), "FREE_FILE_CONTROLS_UNAVAILABLE"),
            (paid.replace("</li></ul>", "<div class='variation-price'>¥ 1,200</div></li></ul>"), "FREE_FILE_CONTROLS_UNAVAILABLE"),
            (paid.replace("class='add-cart'", "class='authored-unknown'"), "FREE_ITEM_ROLE_UNPROVEN"),
            (paid.replace("</ul>", "<li class='variation-item'><div class='variation-price'>¥ 0</div><form><button class='add-cart' data-product-id='900001'>Authored unoffered free row</button></form></li></ul>"), "FREE_FILE_CONTROLS_UNAVAILABLE"),
            (paid.replace("</div></body>", "<div class='text-14 text-text-default font-normal text-left mb-8'>orphan.zip</div></div></body>"), "FREE_ITEM_ORPHAN_ENTRY"),
            (paid.replace("</div></body>", "<span class='cmd-label'>無料ダウンロード</span></div></body>"), "FREE_ITEM_ORPHAN_ENTRY"),
            (paid.replace("</div></body>", "<a href='https://booth.pm/downloadables/700009?variation_id=800009'>Authored orphan</a></div></body>"), "FREE_ITEM_ORPHAN_CONTROL"),
            (paid.replace("</div></body>", "<div data-dropdown-items='[]'></div></div></body>"), "FREE_ITEM_UNKNOWN_DROPDOWN"),
            (paid.replace("<ul id='variations'>", "<ul id='variations' data-file-count='1'>"), "FREE_ITEM_COUNT_UNPROVEN"),
            (paid.replace("</div></body>", "<button aria-expanded='false'></button></div></body>"), "FREE_ITEM_DYNAMIC_LIST"),
            (paid.replace("data-product-id='900001'>Authored donation", "data-product-id='900002'>Authored donation"), "FREE_ITEM_IDENTITY_UNPROVEN"),
            (paid.replace("</ul>", "<li class='variation-item'></li></ul>"), "FREE_ITEM_ROLE_UNPROVEN"),
            (paid.replace("<li class='variation-item'><div class='variation-price'>¥ 1,200</div><form><button class='add-cart' data-product-id='900001'>Authored donation</button></form></li>", ""), "FREE_ITEM_LIST_UNPROVEN"),
        ] {
            let evidence = parse(&html, "900001").unwrap();
            assert_eq!(evidence.error_code.as_deref(), Some(code));
            assert!(!evidence.unavailable);
            assert!(!evidence.manifest_complete);
        }
    let free = parse(HTML, "900001").unwrap();
    assert!(free.manifest_complete);
    assert!(!free.unavailable);
    assert_eq!(free.files.len(), 1);
}
#[test]
fn priced_paused_or_restock_rows_preserve_all_proven_shared_wrap_files() {
    let base = shared_wrap_html_with_files(7);
    let expected = parse(&base, "900001").unwrap();
    assert!(expected.manifest_complete);
    assert_eq!(expected.files.len(), 7);
    let expected_files = serde_json::to_value(&expected.files).unwrap();
    for role in [
            "<button class='add-cart disabled' type='button'>Authored paused offer</button>",
            "<div class='restock-request-links'><a class='js-add-restock-request'>Authored notification</a><a class='js-remove-restock-request'>Authored removal</a></div>",
        ] {
            let row = format!("<li class='variation-item'><div class='variation-price'>¥ 1,200</div>{role}</li>");
            let html = base.replace("</ul>", &format!("{row}</ul>"));
            let evidence = parse(&html, "900001").unwrap();
            assert!(evidence.manifest_complete);
            assert!(!evidence.unavailable);
            assert!(evidence.error_code.is_none());
            assert_eq!(serde_json::to_value(&evidence.files).unwrap(), expected_files);
            for price in ["¥ 0", "¥ 1,200–2,000", "$ 1,200", "1200", "¥ 12,00"] {
                let invalid = html.replace("¥ 1,200", price);
                let evidence = parse(&invalid, "900001").unwrap();
                assert_eq!(evidence.error_code.as_deref(), Some("FREE_ITEM_ROLE_UNPROVEN"), "{price}");
                assert!(!evidence.manifest_complete);
                assert!(!evidence.unavailable);
            }
            for invalid in [
                html.replace("class='variation-price'>¥ 1,200", "class='authored-other-price'>¥ 1,200"),
                html.replace("</li></ul>", "<div class='variation-price'>¥ 1,200</div></li></ul>"),
                html.replace(role, "<button class='authored-unknown'>Authored unknown role</button>"),
            ] {
                assert_eq!(parse(&invalid, "900001").unwrap().error_code.as_deref(), Some("FREE_ITEM_ROLE_UNPROVEN"));
            }
            for (invalid, code) in [
                (html.replace("</div></body>", "<div class='text-14 text-text-default font-normal text-left mb-8'>orphan.zip</div></div></body>"), "FREE_ITEM_ORPHAN_ENTRY"),
                (html.replace("</div></body>", "<a href='https://booth.pm/downloadables/700009?variation_id=800009'>Authored orphan</a></div></body>"), "FREE_ITEM_ORPHAN_CONTROL"),
                (html.replace("</div></body>", "<div data-dropdown-items='[]'></div></div></body>"), "FREE_ITEM_UNKNOWN_DROPDOWN"),
                (html.replace("<ul id='variations'>", "<ul id='variations' data-file-count='8'>"), "FREE_ITEM_COUNT_UNPROVEN"),
                (html.replace("</div></body>", "<button aria-expanded='false'></button></div></body>"), "FREE_ITEM_DYNAMIC_LIST"),
            ] {
                let evidence = parse(&invalid, "900001").unwrap();
                assert_eq!(evidence.error_code.as_deref(), Some(code));
                assert!(!evidence.manifest_complete);
            }
        }
}
#[test]
fn proven_download_controls_do_not_depend_on_display_labels() {
    let original = "<span class='cmd-label'>無料ダウンロード</span>";
    for html in [HTML.to_owned(), shared_wrap_html()] {
        let expected = parse(&html, "900001").unwrap();
        assert!(expected.manifest_complete);
        let expected_files = serde_json::to_value(&expected.files).unwrap();
        for label in [
                "<span class='cmd-label'>ダウンロード</span>",
                "<span class='cmd-label'>Download</span>",
                "",
                "<span class='cmd-label'>無料ダウンロード</span><span class='cmd-label'>Download</span>",
                "<span class='cmd-label'>authored purchase</span>",
                "<span class='cmd-label'>Authored unavailable</span>",
            ] {
                let evidence = parse(&html.replace(original, label), "900001").unwrap();
                assert!(evidence.manifest_complete);
                assert!(evidence.error_code.is_none());
                assert!(!evidence.unavailable);
                assert_eq!(serde_json::to_value(&evidence.files).unwrap(), expected_files);
                assert_eq!(evidence.page().unwrap().unresolved, 0);
            }
    }
}
#[test]
fn shared_cart_wrap_requires_a_complete_filename_control_bijection() {
    let html = shared_wrap_html();
    let proof = parse(&html, "900001").unwrap();
    assert!(proof.manifest_complete);
    assert_eq!(proof.files.len(), 4);
    for i in 1..=4 {
        let file = proof
            .files
            .iter()
            .find(|f| f.id == format!("70000{i}"))
            .unwrap();
        assert_eq!(
            file.variant_id.as_deref(),
            Some(format!("80000{i}").as_str())
        );
        assert_eq!(
            file.name.as_deref(),
            Some(format!("authored-{i}.zip").as_str())
        );
        assert_eq!(file.revision, "unknown");
    }
    let name_node =
        "<div class='text-14 text-text-default font-normal text-left mb-8'>authored-4.zip</div>";
    for bad in [
        html.replace("title='authored-2.zip'", "title='authored-1.zip'"),
        html.replace("authored-2.zip</div>", "authored-1.zip</div>"),
        html.replace("title='authored-1.zip'", "title='authored-2.zip'"),
        html.replace("title='authored-1.zip'", ""),
        html.replace(name_node, ""),
        html.replace(name_node, &format!("{name_node}{name_node}")),
        html.replace("title='authored-4.zip'", "title='unmatched.zip'"),
        html.replace("700002?variation_id=800002", "700001?variation_id=800001"),
        html.replace(
            "700001?variation_id=800001",
            "700001?token=private&variation_id=800001",
        ),
        html.replace(
            name_node,
            &format!("<div class='cart-button-wrap'>{name_node}</div>"),
        ),
        html.replace(
            "<a title='authored-1.zip'",
            "<div class='cart-button-wrap'><a title='authored-1.zip'",
        )
        .replacen("</a>", "</a></div>", 1),
        html.replace(
            "</ul>",
            "<button class='add-cart' data-product-id='900001'></button></ul>",
        )
        .replace(
            "authored-4.zip</div>",
            "authored-4.zip</div><button class='add-cart' data-product-id='900001'></button>",
        ),
    ] {
        let proof = parse(&bad, "900001").unwrap();
        assert!(!proof.manifest_complete);
        assert!(proof.files.is_empty());
    }
    // Equal counts and two distinct names must not let A/A titles consume A twice.
    let ambiguous = html.replace("title='authored-2.zip'", "title='authored-1.zip'");
    assert_eq!(
        parse(&ambiguous, "900001").unwrap().error_code.as_deref(),
        Some("FREE_ITEM_FILENAME_UNPROVEN")
    );
}
#[test]
fn shared_wrap_menus_cannot_grant_or_cross_file_scopes() {
    let html = shared_wrap_html();
    let menu = r#"<div class='js-download-free-button' data-test='other-downloads-button' data-dropdown-items='[{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?client=booth-library-manager&variation_id=800001"}]'></div>"#;
    assert!(html.contains(menu));
    let two_same_file_methods = html.replacen("}]'", r#"},{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?client=vroid&variation_id=800001&type=zip"}]'"#, 1);
    assert!(
        parse(&two_same_file_methods, "900001")
            .unwrap()
            .manifest_complete
    );
    for bad in [
            html.replace("700001/deeplink", "700099/deeplink"),
            html.replace("variation_id=800001\"", "variation_id=800099\""),
            html.replace("client=booth-library-manager", "client=unknown"),
            html.replace("client=booth-library-manager", "client=vroid&type=unknown"),
            html.replace("client=booth-library-manager", "client=vroid&client=booth-library-manager"),
            html.replace("variation_id=800001\"", "no_variant=800001\""),
            html.replace("js-download-free-button", "unknown-method"),
            html.replace("client=booth-library-manager", "token=private&client=booth-library-manager"),
            html.replace("700001/deeplink", "700001/../700001/deeplink"),
            html.replace("700001/deeplink", "%37%30%30%30%30%31/deeplink"),
            html.replace("https://booth.pm/downloadables/700001/deeplink", "https://evil.test/downloadables/700001/deeplink"),
            html.replace(menu, &format!("<div class='cart-button-wrap'>{menu}</div>")),
            html.replace(menu, "").replace("</ul>", &format!("</ul>{menu}")),
            html.replace("data-dropdown-items='[{", "data-dropdown-items='not-json[{"),
            html.replacen("}]'", r#"},{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700002/deeplink?client=booth-library-manager&variation_id=800002"}]'"#, 1),
        ] {
            let proof = parse(&bad, "900001").unwrap();
            assert!(!proof.manifest_complete);
            assert!(proof.files.is_empty());
        }
    let separate = HTML.replace("</ul>", &format!("<li class='variation-item'><div class='cart-button-wrap'><a title='other.zip' href='https://booth.pm/downloadables/700002?variation_id=800002'><span class='cmd-label'>無料ダウンロード</span></a><div class='text-14 text-text-default font-normal text-left mb-8'>other.zip</div>{menu}</div></li></ul>"));
    assert_eq!(
        parse(&separate, "900001").unwrap().error_code.as_deref(),
        Some("FREE_ITEM_UNKNOWN_DROPDOWN")
    );
}
#[test]
fn same_item_canonical_and_og_keep_product_and_list_guards() {
    let expected = "https://booth.pm/ja/items/900001";
    for alias in [
        "https://synthetic-shop.booth.pm/items/900001",
        "https://synthetic-shop.booth.pm/ja/items/900001",
        "https://synthetic-shop.booth.pm/en/items/900001",
    ] {
        let html = HTML.replacen(expected, alias, 1);
        assert!(parse(&html, "900001").unwrap().manifest_complete);
        let og = html.replace(
            "content='https://booth.pm/ja/items/900001'",
            &format!("content='{alias}'"),
        );
        assert!(parse(&og, "900001").unwrap().manifest_complete, "{alias}");
        for bad in [
            html.replace("data-product-id='900001'", "data-product-id='900002'"),
            html.replace("<ul id='variations'>", "<ul id='other'>"),
            html.replace(
                "<ul id='variations'>",
                "<ul id='variations' data-item-id='900002'>",
            ),
            html.replace(
                "</head>",
                &format!("<link rel='canonical' href='{alias}'></head>"),
            ),
            og.replace(
                &format!("content='{alias}'"),
                "content='https://synthetic-shop.booth.pm/items/900002'",
            ),
        ] {
            assert!(!parse(&bad, "900001").unwrap().manifest_complete);
        }
    }
    for (alias, same_item) in [
        ("https://booth.pm/items/900001", true),
        ("https://booth.pm/en/items/900001", true),
        ("https://www.booth.pm/items/900001", true),
        ("https://synthetic-shop.booth.pm:443/items/900001", true),
        ("https://synthetic-shop.booth.pm/items/900001?q=1", true),
        ("https://synthetic-shop.booth.pm/items/900001#x", true),
        ("https://synthetic-shop.booth.pm/ja/../items/900001", true),
        ("https://accounts.booth.pm/items/900001", false),
        ("https://manage.booth.pm/items/900001", false),
        ("https://synthetic-shop.booth.pm/items/900002", false),
        ("https://user@synthetic-shop.booth.pm/items/900001", false),
        (
            "https://synthetic-shop.booth.pm/items/%39%30%30%30%30%31",
            false,
        ),
        ("https://synthetic-shop.booth.pm/items/900001/", false),
        (
            "https://synthetic-shop.booth.pm.evil.test/items/900001",
            false,
        ),
        ("http://synthetic-shop.booth.pm/items/900001", false),
    ] {
        let proof = parse(&HTML.replacen(expected, alias, 1), "900001").unwrap();
        assert_eq!(proof.manifest_complete, same_item, "{alias}");
        if !same_item {
            assert_eq!(
                proof.error_code.as_deref(),
                Some("FREE_ITEM_IDENTITY_UNPROVEN")
            );
        }
    }
}
#[test]
fn strict_free_controls_bind_identity_names_and_variants_without_acquisitions() {
    let second="<li class='variation-item'><div class='cart-button-wrap'><a title='docs.psd' href='https://booth.pm/downloadables/700002?variation_id=800002'><span class='cmd-label'>無料ダウンロード</span></a><div class='text-14 text-text-default font-normal text-left mb-8'>docs.psd</div></div></li>";
    let evidence = parse(&HTML.replace("</ul>", &format!("{second}</ul>")), "900001").unwrap();
    assert!(evidence.manifest_complete);
    assert_eq!(evidence.files.len(), 2);
    assert!(evidence.files.iter().all(|f| f.revision == "unknown"));
    let page = evidence.page().unwrap();
    assert!(page.rows.is_empty());
    assert!(page.is_detail);
    assert!(!page.terminal);
    assert_eq!(page.unresolved, 0);
}
#[test]
fn unknown_or_incomplete_free_controls_never_certify_an_empty_manifest() {
    let first_row = HTML
        .split_once("<li class='variation-item'>")
        .unwrap()
        .1
        .split_once("</li>")
        .unwrap()
        .0;
    let duplicate = format!("<li class='variation-item'>{first_row}</li>");
    for html in [
            HTML.replace("</ul>", &format!("{duplicate}</ul>")),
            HTML.replace("<body>", "<body><div id='items' class='market' data-product-id='900001'></div>"),
            HTML.replace("</ul>", "</ul><a title='orphan.zip' href='https://booth.pm/downloadables/700003?variation_id=800003'><span class='cmd-label'>無料ダウンロード</span></a>"),
            HTML.replace("<li class='variation-item'>", "<li class='variation-item' data-variation-id='800009'>"),
            HTML.replace("<li class='variation-item'>", "<li class='variation-item' data-product-id='900002'>"),
            HTML.replace("</ul>", "<li class='variation-item'><select name='unrecognized'></select></li></ul>"),
            HTML.replace("data-product-id='900001'","data-product-id='900002'"),
            HTML.replace("<ul id='variations'>","<ul id='variations'><ul id='variations'></ul>"),
            HTML.replace("authored.zip</div>","other.zip</div>"),
            HTML.replace("?variation_id=800001", "?token=PRIVATE_TOKEN&variation_id=800001"),
            HTML.replace("?variation_id=800001", ""),
            HTML.replace("</ul>", "<li class='variation-item'><span class='cmd-label'>無料ダウンロード</span></li></ul>"),
            HTML.replace("</ul>", "<li class='variation-item'><div class='text-14 text-text-default font-normal text-left mb-8'>orphan.zip</div></li></ul>"),
            HTML.replace("</ul>", "<button aria-expanded='false'></button></ul>"),
            HTML.replace("<ul id='variations'>", "<ul id='variations' data-file-count='2'>"),
            HTML.replace("</ul>", "<div data-dropdown-items='[]'></div></ul>"),
            HTML.replace("https://booth.pm/downloadables/700001?variation_id=800001", "https://evil.test/downloadables/700001?variation_id=800001"),
            HTML.replace("<a title=", "<a data-href='https://booth.pm/downloadables/700001' title="),
            HTML.replace("</head>", "<link rel='canonical' href='https://booth.pm/ja/items/900001'></head>"),
            HTML.replace("<li class='variation-item'>", "<li>"),
        ] {
            let evidence=parse(&html,"900001").unwrap(); assert!(!evidence.manifest_complete); assert!(evidence.files.is_empty()); assert!(evidence.error_code.is_some()); assert_eq!(evidence.page().unwrap().unresolved,1);
        }
    let absent = HTML.replace("<ul id='variations'>", "<ul id='changed'>");
    assert!(!parse(&absent, "900001").unwrap().manifest_complete);
    assert!(parse(HTML, "900002").unwrap().files.is_empty());
}
#[test]
fn variation_list_identity_cannot_contradict_expected_product() {
    for attribute in ["data-product-id", "data-item-id"] {
        let conflict = HTML.replace(
            "<ul id='variations'>",
            &format!("<ul id='variations' {attribute}='900002'>"),
        );
        let evidence = parse(&conflict, "900001").unwrap();
        assert!(!evidence.manifest_complete);
        assert!(evidence.files.is_empty());
        assert_eq!(
            evidence.error_code.as_deref(),
            Some("FREE_ITEM_IDENTITY_UNPROVEN")
        );
        let matching = HTML.replace(
            "<ul id='variations'>",
            &format!("<ul id='variations' {attribute}='900001'>"),
        );
        assert!(parse(&matching, "900001").unwrap().manifest_complete);
    }
}
#[test]
fn dropdown_outside_variation_list_cannot_hide_an_unobserved_file() {
    let orphan = r#"<div data-dropdown-items='[{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700099/deeplink?client=booth-library-manager&variation_id=800099"}]'></div>"#;
    let html = HTML.replace("</ul>", &format!("</ul>{orphan}"));
    let evidence = parse(&html, "900001").unwrap();
    assert!(!evidence.manifest_complete);
    assert!(evidence.files.is_empty());
    assert_eq!(
        evidence.error_code.as_deref(),
        Some("FREE_ITEM_UNKNOWN_DROPDOWN")
    );
    assert_eq!(evidence.page().unwrap().unresolved, 1);
}
#[test]
fn alternative_methods_are_bound_to_the_same_observed_file() {
    let method = r#"<div class='js-download-free-button' data-test='other-downloads-button' data-dropdown-items='[{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700001/deeplink?client=booth-library-manager&variation_id=800001"}]'></div>"#;
    let html = HTML.replace("authored.zip</div>", &format!("authored.zip</div>{method}"));
    assert!(parse(&html, "900001").unwrap().manifest_complete);
    assert!(
        !parse(
            &html.replace("700001/deeplink", "700002/deeplink"),
            "900001"
        )
        .unwrap()
        .manifest_complete
    );
    assert!(
        !parse(
            &html.replace("variation_id=800001\"", "variation_id=800002\""),
            "900001"
        )
        .unwrap()
        .manifest_complete
    );
}
#[test]
fn vroid_methods_type_only_their_own_file() {
    let menu = |file: &str, kind: &str| {
        format!(
            r#"<div class='js-download-free-button' data-test='other-downloads-button' data-dropdown-items='[{{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/{file}/deeplink?client=booth-library-manager&variation_id=800001"}},{{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/{file}/deeplink?client=vroid&type={kind}&variation_id=800001"}}]'></div>"#
        )
    };
    let with = |name: &str, kind: &str| {
        HTML.replace("authored.zip", name).replace(
            &format!("{name}</div>"),
            &format!("{name}</div>{}", menu("700001", kind)),
        )
    };
    for (name, kind) in [
        ("authored.vroidcustomitem", "vroidcustomitem"),
        ("authored.vroid", "vroid"),
        ("Authored.VROID", "vroid"),
    ] {
        let evidence = parse(&with(name, kind), "900001").unwrap();
        assert!(evidence.manifest_complete && evidence.error_code.is_none());
        assert_eq!(evidence.files.len(), 1);
        assert_eq!(evidence.files[0].name.as_deref(), Some(name));
    }
    for (name, kind) in [
        ("authored.vroidcustomitem", "zip"),
        ("authored.vroidcustomitem", "vroid"),
        ("authored.vroid", ""),
        ("authored", "authored"),
        ("authored.", ""),
    ] {
        let evidence = parse(&with(name, kind), "900001").unwrap();
        assert_eq!(
            evidence.error_code.as_deref(),
            Some("FREE_ITEM_UNKNOWN_DROPDOWN"),
            "{name} {kind}"
        );
        assert!(evidence.files.is_empty());
    }
    // In a shared wrap the type follows the menu's own file, not a neighbour.
    let shared =
        shared_wrap_html_with_files(2).replace("authored-2.zip", "authored-2.vroidcustomitem");
    let typed = |kind: &str| {
        shared.replace("variation_id=800002\"}]", &format!(r#"variation_id=800002"}},{{"deeplinkDownloadableUrl":"https://booth.pm/downloadables/700002/deeplink?client=vroid&type={kind}&variation_id=800002"}}]"#))
    };
    assert_eq!(
        parse(&typed("vroidcustomitem"), "900001")
            .unwrap()
            .files
            .len(),
        2
    );
    assert_eq!(
        parse(&typed("zip"), "900001")
            .unwrap()
            .error_code
            .as_deref(),
        Some("FREE_ITEM_UNKNOWN_DROPDOWN")
    );
}
