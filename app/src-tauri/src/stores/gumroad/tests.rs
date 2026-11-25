use super::*;
use serde_json::json;

#[test]
fn inertia_login_page_requires_authentication_without_accepting_unknown_pages() {
    let html = |page: Value| {
        let payload = serde_json::to_string(&page)
            .unwrap()
            .replace('&', "&amp;")
            .replace('"', "&quot;");
        format!("<div data-page=\"{payload}\"></div>")
    };
    let login = json!({"component":"Logins/New","url":"/login","props":{}});
    assert_eq!(
        inertia_page(&html(login.clone()))
            .err()
            .unwrap()
            .to_string(),
        "AUTH_REQUIRED"
    );
    let unknown = json!({"component":"Unknown/Page","url":"/unknown","props":{}});
    assert_eq!(
        inertia_page(&html(unknown)).err().unwrap().to_string(),
        "GUMROAD_SOURCE_UNSUPPORTED"
    );
    let mut malformed = login;
    malformed["props"] = Value::Null;
    assert_eq!(
        inertia_page(&html(malformed)).err().unwrap().to_string(),
        "GUMROAD_SOURCE_UNSUPPORTED"
    );
    assert_eq!(
        inertia_page(&" ".repeat(LIMIT + 1))
            .err()
            .unwrap()
            .to_string(),
        "GUMROAD_METADATA_TOO_LARGE"
    );
}

#[test]
fn inertia_reads_the_app_root_page_beside_other_data_page_attributes() {
    let attribute = |page: Value| {
        serde_json::to_string(&page)
            .unwrap()
            .replace('&', "&amp;")
            .replace('"', "&quot;")
    };
    // The echoed page URL is absent; a rendered child carries another payload.
    let root = attribute(json!({"component":"Library/Index","props":library()}));
    let child = attribute(json!({"component":"Logins/New","url":"/login","props":{}}));
    let html = format!(
            "<meta data-page=\"library\"><div id=\"app\" data-page=\"{root}\"><span data-page=\"{child}\"></span></div>"
        );
    let page = inertia_page(&html).unwrap();
    assert_eq!(page["component"], "Library/Index");
    assert_eq!(page["props"]["pagination"]["count"], 569);
}

fn library() -> Value {
    let results: Vec<Value> = (0..15).map(|index| json!({"product":{"name":"Authored fixture","native_type":"digital"},"purchase":{"id":format!("authored-purchase-{index}"),"is_archived":false,"download_url":"/d/authored-capability","variants":"Standard"}})).collect();
    json!({"results":results,"logged_in_user":{"id":"authored-buyer"},
            "pagination":{"page":1,"pages":38,"count":569,"from":1,"to":15},"archived_count":3,"unarchived_count":574,
            "search":{"sort":"purchase_date","query":"","creators":[],"bundles":[],"show_archived_only":false}})
}
fn content() -> Value {
    json!({"purchase":{"id":"authored-purchase","receipt_purchase_id":"authored-purchase","product_id":"authored-product","created_at":"2026-10-08T01:02:03Z","product_available":true},
            "content":{"content_items":[{"type":"folder","id":"authored-folder","name":"Originals","children":[
                {"type":"file","id":"authored-file-one","file_name":"fixture.unitypackage","file_size":42,"pdf_stamp_enabled":false,"content_length":null,"download_url":"/r/authored-capability/product_files?product_file_ids[]=authored-file-one"},
                {"type":"file","id":"authored-file-two","file_name":"guide.pdf","file_size":0,"pdf_stamp_enabled":false,"content_length":7,"download_url":"/r/authored-capability/product_files?product_file_ids[]=authored-file-two"}]}]}})
}

#[test]
fn public_price_keeps_exact_product_and_currency_units_separate_from_paid_evidence() {
    let mut value = json!({"api_version":1,"id":"authored-product","price_cents":19900,"currency_code":"usd","price_formatted":"$199","is_pay_what_you_want":false,"is_recurring_billing":false});
    let public = parse_public_product(&value, Some("authored-product")).unwrap();
    assert_eq!(public.current_price.unwrap().usd_amount(), Some(199.0));
    assert!(parse_public_product(&value, Some("other-product")).is_err());
    value["price_cents"] = json!(0);
    assert_eq!(
        parse_public_product(&value, None)
            .unwrap()
            .current_price
            .unwrap()
            .usd_amount(),
        Some(0.0)
    );
    value["currency_code"] = json!("jpy");
    value["price_cents"] = json!(1000);
    value["price_formatted"] = json!("¥1,000");
    let public = parse_public_product(&value, None).unwrap();
    assert!(public.current_price.unwrap().usd_amount().is_none());
    assert_eq!(public.price_formatted.as_deref(), Some("¥1,000"));
    assert_eq!(public.price_source_currency.as_deref(), Some("JPY"));
    value["currency_code"] = json!("usd");
    value["price_cents"] = json!(9_007_199_254_740_992u64);
    value["price_formatted"] = json!("$1\nscript");
    let public = parse_public_product(&value, None).unwrap();
    assert!(public.current_price.unwrap().usd_amount().is_none());
    assert!(public.price_formatted.is_none());
}

#[test]
fn public_description_observation_distinguishes_unknown_empty_and_visible_text() {
    let mut value = json!({"api_version":1,"id":"authored-product"});
    assert!(parse_public_product(&value, None)
        .unwrap()
        .description
        .is_none());
    for unknown in [Value::Null, json!({"text":"VRChat"}), json!(42)] {
        value["description"] = unknown;
        assert!(parse_public_product(&value, None)
            .unwrap()
            .description
            .is_none());
    }
    for empty in ["", " \n\t", "<p></p>"] {
        value["description"] = json!(empty);
        assert_eq!(
            parse_public_product(&value, None)
                .unwrap()
                .description
                .as_deref(),
            Some("")
        );
    }
    value["description"] = json!("<p>Authored &amp; decoded</p><p>For VR<strong>Chat</strong><br>VRC &#x8863;&#x88c5;</p><a href='https://example.invalid/VRChat' title='VRChat'>Read manual</a><img alt='VRChat'><script>VRChat</script><style>VRC</style><nav>VRChat</nav><template>VRChat</template><!-- VRC -->");
    assert_eq!(
        parse_public_product(&value, None)
            .unwrap()
            .description
            .as_deref(),
        Some("Authored & decoded For VRChat VRC 衣装 Read manual")
    );
    value["description"] = json!("<a href='https://example.invalid/VRChat' title='VRChat'>Read manual</a><img alt='VRChat'><script>VRChat</script><style>VRC</style><nav>VRChat</nav>");
    assert_eq!(
        parse_public_product(&value, None)
            .unwrap()
            .description
            .as_deref(),
        Some("Read manual")
    );
    value["description"] = json!("&lt;literal&gt; &amp;copy; VRChat");
    assert_eq!(
        parse_public_product(&value, None)
            .unwrap()
            .description
            .as_deref(),
        Some("<literal> &copy; VRChat")
    );
    for blocks in ["<p>VR</p><p>Chat</p>", "VR<br>Chat", "VR<hr>Chat"] {
        value["description"] = json!(blocks);
        assert_eq!(
            parse_public_product(&value, None)
                .unwrap()
                .description
                .as_deref(),
            Some("VR Chat")
        );
    }
    value["description"] = json!("猫".repeat(40_001));
    assert_eq!(
        parse_public_product(&value, None)
            .unwrap()
            .description
            .unwrap()
            .chars()
            .count(),
        40_000
    );
}

#[test]
fn buyer_library_rejects_stale_page_filtered_scope_and_duplicate_purchase_identity() {
    let base = Url::parse("https://gumroad.com/library?page=1").unwrap();
    let parsed = parse_library_props(&library(), &base, 1, false).unwrap();
    assert_eq!(parsed.pages, 38);
    assert_eq!(parsed.purchases[0].purchase_id, "authored-purchase-0");
    assert!(parsed.purchases[0].content_route.is_some());
    assert!(parse_library_props(&library(), &base, 2, false).is_err());
    assert!(parse_library_props(&library(), &base, 1, true).is_err());
    let mut filtered = library();
    filtered["search"]["creators"] = json!(["authored-creator"]);
    assert!(parse_library_props(&filtered, &base, 1, false).is_err());
    let mut duplicate = library();
    duplicate["results"][1] = library()["results"][0].clone();
    assert!(parse_library_props(&duplicate, &base, 1, false).is_err());
    let mut truncated = library();
    truncated["results"].as_array_mut().unwrap().pop();
    assert!(parse_library_props(&truncated, &base, 1, false).is_err());
    let mut empty_archive = library();
    empty_archive["results"] = json!([]);
    empty_archive["pagination"] = json!({"page":1,"pages":1,"from":0,"to":0,"count":0});
    empty_archive["archived_count"] = json!(0);
    empty_archive["search"]["show_archived_only"] = json!(true);
    let empty = parse_library_props(&empty_archive, &base, 1, true).unwrap();
    assert_eq!(empty.pages, 1);
    assert!(empty.purchases.is_empty());
    for native_type in ["physical", "call", "coffee", "commission"] {
        let mut non_asset = library();
        non_asset["results"][0]["product"]["native_type"] = json!(native_type);
        let page = parse_library_props(&non_asset, &base, 1, false).unwrap();
        assert!(page.purchases[0].excluded_non_asset());
        assert!(!page.purchases[1].excluded_non_asset());
    }
}

#[test]
fn buyer_library_reads_pages_whose_echoes_sort_totals_or_one_card_route_changed() {
    let base = Url::parse("https://gumroad.com/library?page=1").unwrap();
    let mut changed = library();
    changed["search"] = json!({"sort":"authored_new_order"});
    for key in ["archived_count", "unarchived_count"] {
        changed.as_object_mut().unwrap().remove(key);
    }
    changed["results"][0]["purchase"]
        .as_object_mut()
        .unwrap()
        .remove("is_archived");
    changed["results"][1]["purchase"]["download_url"] =
        json!("https://authored.invalid/d/authored-capability");
    changed["results"][2]["purchase"]["download_url"] = json!(42);
    let page = parse_library_props(&changed, &base, 1, false).unwrap();
    assert_eq!(page.purchases.len(), 15);
    assert!(!page.purchases[0].archived);
    assert!(page.purchases[1].content_route.is_none());
    assert!(page.purchases[2].content_route.is_none());
    assert!(page.purchases[3].content_route.is_some());
    // An echo or row that shows another tab or a filter still fails the page.
    for (key, value) in [
        ("show_archived_only", json!(true)),
        ("query", json!("authored")),
        ("creators", json!(["authored-creator"])),
        ("bundles", json!(["authored-bundle"])),
    ] {
        let mut filtered = changed.clone();
        filtered["search"][key] = value;
        assert_eq!(
            parse_library_props(&filtered, &base, 1, false)
                .err()
                .unwrap()
                .to_string(),
            "GUMROAD_SCOPE_CHANGED"
        );
    }
    let mut other_tab = changed.clone();
    other_tab["results"][3]["purchase"]["is_archived"] = json!(true);
    assert!(parse_library_props(&other_tab, &base, 1, false).is_err());
    // Only the echo proves that member cards belong to the requested bundle.
    changed["bundles"] = json!([{"id":"authored-bundle"}]);
    assert!(parse_library_scope(&changed, &base, 1, false, Some("authored-bundle")).is_err());
    changed["search"]["bundles"] = json!(["authored-bundle"]);
    assert!(parse_library_scope(&changed, &base, 1, false, Some("authored-bundle")).is_ok());
}

#[test]
fn content_binds_purchase_product_and_every_nested_file_without_using_bearer_identity() {
    let base = Url::parse("https://gumroad.com/d/authored-capability").unwrap();
    let parsed = parse_content_props(
        &content(),
        &base,
        "authored-purchase",
        Some("authored-product"),
    )
    .unwrap();
    assert!(parsed.inventory_complete);
    assert_eq!(parsed.catalogue_product_id(), "gumroad:authored-product");
    assert_eq!(parsed.files.len(), 2);
    assert_eq!(parsed.files[1].size_bytes, Some(0));
    assert_eq!(parsed.files[0].folders, ["Originals"]);
    assert_eq!(
        parsed.acquired_at.unwrap().date_naive().to_string(),
        "2026-10-08"
    );
    assert!(parse_content_props(&content(), &base, "other-purchase", None).is_err());
    assert!(parse_content_props(
        &content(),
        &base,
        "authored-purchase",
        Some("other-product")
    )
    .is_err());
    let mut unknown = content();
    unknown["content"]["content_items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"unknown-file-control"}));
    let parsed = parse_content_props(&unknown, &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert_eq!(parsed.files.len(), 2);
    assert!(parsed.acquired_at.is_some());

    let reference = json!({"type":"file","id":"authored-external-reference","file_name":"External reference","external_link_url":"https://example.invalid/authored-reference"});
    let mut mixed = content();
    mixed["content"]["content_items"]
        .as_array_mut()
        .unwrap()
        .push(reference.clone());
    let mixed = parse_content_props(&mixed, &base, "authored-purchase", None).unwrap();
    assert!(mixed.inventory_complete);
    assert_eq!(mixed.unsupported_items, 0);
    assert_eq!(
        mixed
            .files
            .iter()
            .filter(|file| file.access == FileAccess::Downloadable)
            .map(|file| file.file_id.as_str())
            .collect::<Vec<_>>(),
        ["authored-file-one", "authored-file-two"]
    );
    assert_eq!(mixed.files[2].access, FileAccess::ExternalReference);
    assert_eq!(mixed.files[2].name.as_deref(), Some("External reference"));
    assert!(mixed.files[2].resolver.is_none());
    assert_eq!(mixed.acquired_at, parsed.acquired_at);

    let mut external_only = content();
    external_only["content"]["content_items"] = json!([reference]);
    let external_only =
        parse_content_props(&external_only, &base, "authored-purchase", None).unwrap();
    assert!(external_only.inventory_complete);
    assert_eq!(external_only.unsupported_items, 0);
    assert!(external_only
        .files
        .iter()
        .all(|file| file.access == FileAccess::ExternalReference && file.resolver.is_none()));
    assert!(external_only.acquired_at.is_some());
}

#[test]
fn byte_length_uses_file_size_and_stamped_files_keep_identity_without_claiming_original_size() {
    let base = Url::parse("https://gumroad.com/d/authored-capability").unwrap();
    let mut value = content();
    let media = &mut value["content"]["content_items"][0]["children"][0];
    media["file_name"] = json!("authored.mp3");
    media["file_size"] = json!(65536);
    media["content_length"] = json!(240);
    let parsed = parse_content_props(&value, &base, "authored-purchase", None).unwrap();
    assert_eq!(parsed.files[0].size_bytes, Some(65536));
    value["content"]["content_items"][0]["children"][1]["pdf_stamp_enabled"] = json!(true);
    let stamped = parse_content_props(&value, &base, "authored-purchase", None).unwrap();
    assert!(stamped.inventory_complete);
    assert_eq!(stamped.files[0].size_bytes, Some(65536));
    assert!(stamped.files[1].size_bytes.is_none());
    assert_eq!(stamped.files[1].file_id, parsed.files[1].file_id);
    assert_eq!(stamped.files[1].access, FileAccess::Downloadable);
    assert_eq!(stamped.acquired_at, parsed.acquired_at);
}

#[test]
fn unsafe_or_ambiguous_file_controls_keep_bound_date_but_never_complete_inventory() {
    let base = Url::parse("https://gumroad.com/d/authored-capability").unwrap();
    let mut duplicate = content();
    let file = duplicate["content"]["content_items"][0]["children"][0].clone();
    duplicate["content"]["content_items"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(file);
    let parsed = parse_content_props(&duplicate, &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert!(parsed.files[0].resolver.is_none());
    assert!(parsed.acquired_at.is_some());
    let mut unsafe_file = content();
    unsafe_file["content"]["content_items"][0]["children"][0]["download_url"] =
        json!("https://gumroad.com.evil.test/fixture.zip");
    let parsed = parse_content_props(&unsafe_file, &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert!(parsed.files[0].resolver.is_none());
    assert!(parsed.acquired_at.is_some());
    unsafe_file["content"]["content_items"][0]["children"][0]["external_link_url"] =
        json!("https://example.invalid/authored-reference");
    let parsed = parse_content_props(&unsafe_file, &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert_eq!(parsed.files[0].access, FileAccess::ExternalReference);
    assert!(parsed.files[0].resolver.is_none());
    assert!(parsed.acquired_at.is_some());
    let public = json!({"api_version":1,"id":"authored-product","name":"Fixture","price_cents":0,"currency_code":"usd","is_pay_what_you_want":true,"is_recurring_billing":false,"created_at":"2000-01-01T00:00:00Z"});
    let quote = parse_public_product(&public, Some("authored-product"))
        .unwrap()
        .current_price
        .unwrap();
    assert_eq!(quote.price_cents, 0);
    assert!(quote.pay_what_you_want);
    assert_eq!(quote.currency_code, "USD");
    assert!(parse_public_product(&public, Some("other-product")).is_err());
}

#[test]
fn inertia_allowlist_discards_shared_user_tokens_license_and_unrelated_private_props() {
    let page = json!({"component":"Library/Index","url":"/library","props":library()});
    let mut page = page;
    page["props"]["user"] = json!({"email":"authored-sensitive","token":"authored-sensitive"});
    page["props"]["token"] = json!("authored-sensitive");
    page["props"]["purchase_analytics"] = json!({"value":"authored-sensitive"});
    let needed = necessary_page(&page).unwrap();
    assert!(!needed.to_string().contains("authored-sensitive"));
    let mut page = json!({"component":"UrlRedirects/DownloadPage","url":"/d/authored-capability","props":content()});
    page["props"]["content"]["license"] = json!("authored-sensitive");
    page["props"]["purchase"]["email"] = json!("authored-sensitive");
    assert!(!necessary_page(&page)
        .unwrap()
        .to_string()
        .contains("authored-sensitive"));
}

#[test]
fn bundle_and_empty_controls_never_certify_only_the_parent_inventory() {
    let base = Url::parse("https://gumroad.com/d/authored-capability").unwrap();
    let mut value = content();
    value["content"]["bundle_products"] =
        json!([{"id":"authored-member","name":"Member","url":"/d/authored-member-capability"}]);
    let page = necessary_page(
        &json!({"component":"UrlRedirects/DownloadPage","url":base.as_str(),"props":value}),
    )
    .unwrap();
    let parsed = parse_content_props(&page["props"], &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert_eq!(parsed.bundles.len(), 1);
    assert_eq!(parsed.files.len(), 2);
    let mut empty = content();
    empty["content"]["content_items"][0]["children"][0]["download_url"] = json!("");
    let parsed = parse_content_props(&empty, &base, "authored-purchase", None).unwrap();
    assert!(!parsed.inventory_complete);
    assert!(parsed.files[0].resolver.is_none());
    let mut impostor = library();
    impostor["logged_in_user"] = Value::Null;
    impostor["current_seller"] = json!({"id":"authored-buyer"});
    assert!(parse_library_props(
        &impostor,
        &Url::parse("https://gumroad.com/library").unwrap(),
        1,
        false
    )
    .is_err());
}

#[test]
fn file_controls_require_one_exact_file_identity_on_the_resolver_route() {
    let base = Url::parse("https://gumroad.com/d/authored-capability").unwrap();
    for route in [
            "/r/authored-capability/product_files?product_file_ids[]=other-file",
            "/r/authored-capability/product_files?product_file_ids[]=authored-file-one&product_file_ids[]=authored-file-two",
            "/r/authored-capability/product_files",
            "/d/authored-capability/download?product_file_ids[]=authored-file-one",
        ] {
            let mut value = content(); value["content"]["content_items"][0]["children"][0]["download_url"] = json!(route);
            let parsed = parse_content_props(&value, &base, "authored-purchase", None).unwrap();
            assert!(!parsed.inventory_complete);
            assert!(parsed.files[0].resolver.is_none());
            assert!(parsed.acquired_at.is_some());
        }
    // Another first-party host, an unrelated parameter or a fragment keeps the binding.
    for route in [
            "/r/authored-capability/product_files?product_file_ids[]=authored-file-one",
            "/r/authored-capability/product_files?product_file_ids[]=authored-file-one&other=true",
            "https://app.gumroad.com/r/authored-capability/product_files?product_file_ids[]=authored-file-one",
            "/r/authored-capability/product_files?product_file_ids[]=authored-file-one#files",
        ] {
            let mut value = content(); value["content"]["content_items"][0]["children"][0]["download_url"] = json!(route);
            let parsed = parse_content_props(&value, &base, "authored-purchase", None).unwrap();
            assert!(parsed.inventory_complete);
            let resolver = parsed.files[0].resolver.as_ref().unwrap().url();
            assert_eq!(resolver.path(), "/r/authored-capability/product_files");
            assert!(resolver.fragment().is_none());
        }
}
