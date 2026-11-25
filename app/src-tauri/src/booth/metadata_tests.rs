use super::*;
#[test]
fn gift_date_comes_only_from_this_items_accepted_field() {
    let item = |id: Value, accepted: Value| json!({"id":id,"gift":{"accepted_at":accepted,"url":"https://booth.pm/gifts/PRIVATE_CLAIM"}});
    for (accepted, expected) in [
        ("2026年11月1日", "2026/11/01"),
        ("2026年1月31日", "2026/01/31"),
        (" 2026年12月25日 ", "2026/12/25"),
        ("2026-11-01", "2026/11/01"),
    ] {
        let value = item(json!(900001), json!(accepted));
        assert_eq!(
            gift_accepted_date(&value, "900001").as_deref(),
            Some(expected)
        );
    }
    assert!(gift_accepted_date(&item(json!("900001"), json!("2026年11月1日")), "900001").is_some());
    for value in [
        item(json!(900002), json!("2026年11月1日")),
        item(json!(900001), json!("2026年2月30日")),
        item(json!(900001), json!("November 1, 2026")),
        item(json!(900001), Value::Null),
        json!({"id":900001,"gift":null}),
        json!({"id":900001,"order":{"purchased_at":"2026年11月1日"}}),
        json!({"gift":{"accepted_at":"2026年11月1日"}}),
    ] {
        assert_eq!(gift_accepted_date(&value, "900001"), None, "{value}");
    }
}
#[test]
fn public_digital_variant_labels_are_preserved_without_files_or_private_fields() {
    let payload = json!({"id":900001,"name":"Authored clothing","variations":[
            {"id":900010,"name":"<b>全部入りフルパック</b>","type":"digital","order_url":"PRIVATE_ORDER","downloadable":{"url":"PRIVATE_SIGNED"}},
            {"id":"900011","name":"Astercat専用","type":"digital"},
            {"id":900012,"name":"Physical version","type":"physical"},
            {"id":"bad-id","name":"Invalid","type":"digital"}]});
    let mut product = public_metadata(&payload, "900001").unwrap();
    assert_eq!(
        product["publicVariants"],
        json!([
            {"id":"900010","name":"全部入りフルパック","type":"digital"},
            {"id":"900011","name":"Astercat専用","type":"digital"}])
    );
    assert!(!product.to_string().contains("PRIVATE"));
    assert!(product.get("files").is_none());
    apply_public_response(
        &mut product,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Sparse authored"})),
    )
    .unwrap();
    assert_eq!(product["publicVariants"].as_array().unwrap().len(), 2);
    apply_public_response(
        &mut product,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Empty authored","variations":[]})),
    )
    .unwrap();
    assert_eq!(product["publicVariants"], json!([]));
}
#[test]
fn public_owned_variation_quotes_keep_zero_and_do_not_allocate_listing_ranges() {
    let product = public_metadata(
        &json!({"id":900001,"name":"Authored variants","price":"¥ 100 - ¥ 300","variations":[
            {"id":800001,"name":"Zero","type":"digital","price":0},
            {"id":800002,"name":"Paid","type":"digital","price":"¥ 300"},
            {"id":800003,"name":"Unknown","type":"digital","price":"¥ 100 - ¥ 300"}
        ]}),
        "900001",
    )
    .unwrap();
    assert_eq!(product["publicVariants"][0]["price"].as_f64(), Some(0.0));
    assert_eq!(product["publicVariants"][1]["price"].as_f64(), Some(300.0));
    assert_eq!(product["publicVariants"][1]["currency"], "JPY");
    assert!(product["publicVariants"][2].get("price").is_none());
    assert!(product["price"].is_null());
}
#[test]
fn unnamed_digital_variations_keep_exact_quotes_without_listing_inference() {
    let mut product = json!({"id":"900001","note":"Human note"});
    apply_public_response(
            &mut product,
            "900001",
            200,
            Some(&json!({"id":900001,"name":"Authored unnamed variations","description":"Authored digital clothing","shop":{"name":"Authored shop","url":"https://invented.booth.pm"},"category":{"id":1,"name":"3D衣装"},"tags":[{"name":"VRChat"}],"images":[],"price":"¥ 100~","variations":[
                {"id":800001,"name":null,"type":"digital","price":6000},
                {"id":"800002","type":"digital","price":0},
                {"id":800003,"name":" ","type":"digital","price":"¥ 100~"},
                {"id":800004,"name":null,"type":"physical","price":100},
                {"id":"invalid","name":null,"type":"digital","price":100}
            ]})),
        )
        .unwrap();
    assert_eq!(
        product["publicVariants"],
        json!([
            {"id":"800001","type":"digital","price":6000.0,"currency":"JPY"},
            {"id":"800002","type":"digital","price":0.0,"currency":"JPY"},
            {"id":"800003","type":"digital"}
        ])
    );
    assert!(product["price"].is_null());
    assert_eq!(product["note"], "Human note");
}
#[test]
fn public_support_sections_use_positive_item_links_and_ignore_other_sections() {
    let mut product = json!({"sourceTitle":"Authored clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"]});
    assert!(needs_public_support_html(&product));
    apply_public_support_html(&mut product,r#"<main>
            <section><h2>Credits</h2><p>真冬 https://booth.pm/ja/items/5007531</p></section>
            <section><h2>◆対応アバター</h2><p>・オリジナル3Dモデル「しなの」
https://ponderogen.booth.pm/items/6106863
・オリジナル3Dモデル「そらねこ」
https://synthetic.booth.pm/items/900099
・マヌカ非対応
https://jingo1016.booth.pm/items/5058077</p></section>
            <section><h2>Dependencies</h2><p>Lapwing https://booth.pm/ja/items/4993931</p></section>
            <section><h2>非対応アバター</h2><p>マリシア https://booth.pm/ja/items/6305948</p></section>
        </main>"#).unwrap();
    let found = product["publicSupportAvatars"].as_array().unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|v| v["id"] == "booth:6106863"));
    assert!(found
        .iter()
        .any(|v| v["id"] == "booth:900099" && v["officialName"] == "そらねこ"));
    assert!(found.iter().all(|v| v["entitlementEstablished"] == false));
    assert!(needs_public_support_html(&product));
    assert!(product.get("html").is_none());
    let prior = product["publicSupportAvatars"].clone();
    apply_public_support_html(
        &mut product,
        "<section><h2>Credits</h2><p>Manuka</p></section>",
    )
    .unwrap();
    assert_eq!(product["publicSupportAvatars"], prior);
    product["sourceCategory"] = json!({"name":"ソフトウェア"});
    product["sourceTitle"] = json!("Authored Blender addon");
    product["publicSupportAvatars"] = json!([]);
    assert!(!needs_public_support_html(&product));
}
#[test]
fn html_only_public_support_refreshes_and_withdraws_stale_bases() {
    let mut product = json!({"sourceTitle":"Authored VRChat clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"],"publicVariants":[{"id":"900010","name":"全部入りフルパック","type":"digital"}]});
    apply_public_support_html(&mut product,"<section class='shop__text'><h2>対応アバター</h2><p>しなの\nマヌカ\nLapwing\nマリシア</p></section>").unwrap();
    assert_eq!(product["publicSupportAvatars"].as_array().unwrap().len(), 4);
    assert!(needs_public_support_html(&product));
    assert_eq!(
        crate::taxonomy::account_variant_avatars(&product, Some("900010"), "").len(),
        4
    );
    apply_public_support_html(
        &mut product,
        "<section class='shop__text'><h2>対応アバター</h2><p>しなの\nマヌカ\nLapwing</p></section>",
    )
    .unwrap();
    assert_eq!(product["publicSupportAvatars"].as_array().unwrap().len(), 3);
    assert_eq!(
        crate::taxonomy::account_variant_avatars(&product, Some("900010"), "").len(),
        3
    );
    assert!(needs_public_support_html(&product));
    let retained = product["publicSupportAvatars"].clone();
    apply_public_support_html(&mut product, "<main>Login required</main>").unwrap();
    assert_eq!(product["publicSupportAvatars"], retained);
    apply_public_support_html(
        &mut product,
        "<section class='shop__text'><h2>対応アバター</h2><p></p></section>",
    )
    .unwrap();
    assert_eq!(product["publicSupportAvatars"], json!([]));
    assert!(crate::taxonomy::account_variant_avatars(&product, Some("900010"), "").is_empty());
    product["publicSupportAvatars"] = retained;
    apply_public_support_html(
        &mut product,
        "<section class='shop__text'><h2>商品概要</h2><p>Authored clothing</p></section>",
    )
    .unwrap();
    assert_eq!(product["publicSupportAvatars"], json!([]));
    assert!(needs_public_support_html(&product));
}
#[test]
fn all_inclusive_offer_refreshes_support_even_with_known_title_bases() {
    let mut product = json!({"sourceTitle":"しなの マヌカ VRChat clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat","しなの","マヌカ"],"publicVariants":[{"id":"900010","name":"全部入りフルパック","type":"digital"}]});
    assert_eq!(crate::taxonomy::listing_avatar_evidence(&product).len(), 2);
    assert!(needs_public_support_html(&product));
    apply_public_support_html(&mut product, "<section class='shop__text'><h2>対応アバター</h2><p>しなの\nマヌカ\nLapwing\nマリシア</p></section>").unwrap();
    assert_eq!(product["publicSupportAvatars"].as_array().unwrap().len(), 4);
    assert_eq!(
        crate::taxonomy::account_variant_avatars(&product, Some("900010"), "").len(),
        4
    );
}
#[test]
fn exact_jpy_prices_require_safe_whole_amounts_and_strict_grouping() {
    for (value, amount) in [
        (json!(0), 0.0),
        (json!(1500), 1500.0),
        (json!(1500.0), 1500.0),
        (json!("¥ 1,500"), 1500.0),
        (json!("￥１，５００"), 1500.0),
        (json!("  1,500円  "), 1500.0),
        (json!("1,500 JPY"), 1500.0),
        (json!("  ０　ＪＰＹ  "), 0.0),
        (json!("1500"), 1500.0),
        (json!("¥ 0"), 0.0),
        (json!(MAX_SAFE_JPY), MAX_SAFE_JPY as f64),
    ] {
        assert_eq!(parse_jpy_price(&value), Some(amount), "{value}");
    }
    for value in [
        json!(MAX_SAFE_JPY + 1),
        json!(-1),
        json!(-0.0),
        json!(1.5),
        json!(true),
        Value::Null,
        json!("無料"),
        json!("NaN"),
        json!("Infinity"),
        json!("¥ -1"),
        json!("+100"),
        json!("1e3"),
        json!("$ 100"),
        json!("USD 100"),
        json!("100 USD"),
        json!("JPY 100"),
        json!("100 jpy"),
        json!("¥ 100 JPY"),
        json!("100 円 JPY"),
        json!("1,50 JPY"),
        json!("01,500 JPY"),
        json!("100.0 JPY"),
        json!("-100 JPY"),
        json!("100 JPY shipping"),
        json!("100 JPY ~ 200 JPY"),
        json!("9007199254740992 JPY"),
        json!("¥ 1,50"),
        json!("¥ 1,500,"),
        json!("¥ 01,500"),
        json!("¥ 1 500"),
        json!("¥ 100.0"),
        json!("¥ 100 円"),
        json!("¥ 100 ~"),
        json!("¥ 100 ~ 200"),
        json!("<b>¥ 100</b>"),
        json!("¥ 100\u{0000}"),
        json!("9".repeat(129)),
    ] {
        assert!(parse_jpy_price(&value).is_none(), "{value}");
    }
}
#[test]
fn bounded_price_display_recognizes_ranges_without_claiming_exact_price() {
    for (value, display) in [
        (json!("¥ 1,500 ~"), "¥ 1,500 ~"),
        (json!("￥１，５００　～"), "¥1,500 ~"),
        (json!("¥ 100 〜 ¥ 1,500"), "¥ 100 〜 ¥ 1,500"),
        (json!("¥ 100 ~ 100"), "¥ 100 ~ 100"),
    ] {
        assert_eq!(listing_price_display(&value).as_deref(), Some(display));
        assert!(parse_jpy_price(&value).is_none());
    }
    assert_eq!(listing_price_display(&json!(0)).as_deref(), Some("¥ 0"));
    assert_eq!(listing_starting_price(&json!("¥ 1,500 ~")), Some(1500.0));
    assert_eq!(
        listing_starting_price(&json!("¥ 100 〜 ¥ 1,500")),
        Some(100.0)
    );
    assert_eq!(listing_starting_price(&json!(0)), Some(0.0));
    assert_eq!(listing_starting_price(&json!("¥ 500 ~ 100")), None);
    for text in [
        "¥ 500 ~ 100",
        "¥ 100 ~~",
        "¥ 100 ~ USD 200",
        "¥ 1,50 ~",
        "~ ¥ 100",
        "無料",
        "<b>¥ 100 ~</b>",
        "¥ 100 ~ shipping",
        "¥ 100 / ¥ 200",
    ] {
        assert!(listing_price_display(&json!(text)).is_none(), "{text}");
    }
}
#[test]
fn public_price_quotes_preserve_source_text_and_never_use_variations() {
    for value in [json!("¥ 1,500"), json!(1500), json!("¥ 1,500 ~")] {
        let public = public_metadata(
            &json!({"id":900001,"name":"Authored","price":value,"variations":[{"price":0}]}),
            "900001",
        )
        .unwrap();
        assert_eq!(public["price"], json!(parse_jpy_price(&value)));
    }
    let mut product = json!({"id":"900001","price":1500,"listingPriceText":"¥ 1,500"});
    apply_public_response(
        &mut product,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Authored","price":"¥ 1,500 ~"})),
    )
    .unwrap();
    assert!(product["price"].is_null());
    assert_eq!(product["listingPriceText"], "¥ 1,500 ~");
    let range = product.clone();
    assert!(apply_public_response(&mut product, "900001", 503, None).is_err());
    assert_eq!(product, range);
    apply_public_response(
        &mut product,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Authored","price":0})),
    )
    .unwrap();
    assert_eq!(product["price"].as_f64(), Some(0.0));
    apply_public_response(
        &mut product,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Authored"})),
    )
    .unwrap();
    assert_eq!(product["price"].as_f64(), Some(0.0));
}
#[test]
fn only_public_fields_and_safe_urls_survive() {
    let v = json!({"id":900001,"name":"Invented","cookies":"secret","shop":{"name":"Maker","url":"https://invented.booth.pm/?token=bad"},"images":[{"original":"https://booth.pximg.net/test.png?token=bad"}],"tags":[{"name":"Authored"}],"variations":[{"downloadable":{"url":"https://booth.pm/downloadables/7?token=secret"}}]});
    let p = public_metadata(&v, "900001").unwrap();
    assert_eq!(p["sourceUrl"], "https://booth.pm/ja/items/900001");
    let s = p.to_string();
    assert!(!s.contains("secret"));
    assert!(!s.contains("token"));
    assert!(p.get("files").is_none());
    assert!(public_metadata(&v, "900002").is_err());
}
fn retained_product() -> Value {
    json!({"id":"900001","sourceTitle":"Authored VRChat tool","title":"Authored VRChat tool","description":"Retained VRChat Unity description","sourceDescription":"Retained VRChat Unity description","sourceTags":["VRChat","Unity"],"tags":["VRChat","Unity"],"sourceCategory":{"name":"3D tools"},"publicPageAvailability":"available"})
}
#[test]
fn public_description_is_clean_and_bounded() {
    let payload = json!({"id":900001,"name":"<b>Authored avatar</b>","description":"<p>Unity &amp; VRChat</p><script>NOT_CLASSIFICATION</script><style>NOT_CLASSIFICATION</style><p>対応\nモデル\u{0001}</p>","tags":[{"name":"<b>VRChat</b>"}],"category":{"name":"<b>3D</b>"}});
    let public = public_metadata(&payload, "900001").unwrap();
    assert_eq!(public["sourceDescription"], "Unity & VRChat 対応 モデル");
    assert_eq!(public["sourceTitle"], "Authored avatar");
    assert_eq!(public["sourceTags"], json!(["VRChat"]));
    assert_eq!(public["sourceCategory"]["name"], "3D");
    assert_eq!(public["publicPageAvailability"], "available");
    let long = public_metadata(
        &json!({"id":900001,"name":"Invented","description":"界".repeat(50000)}),
        "900001",
    )
    .unwrap();
    assert_eq!(
        long["sourceDescription"].as_str().unwrap().chars().count(),
        40000
    );
}
#[test]
fn deleted_page_retains_last_good_vr_classification_inputs() {
    for status in [404, 410] {
        let mut product = retained_product();
        let before = product.clone();
        apply_public_response(&mut product, "900001", status, None).unwrap();
        for key in [
            "sourceTitle",
            "title",
            "description",
            "sourceDescription",
            "sourceTags",
            "tags",
            "sourceCategory",
        ] {
            assert_eq!(product[key], before[key], "{key}");
        }
        assert_eq!(product["publicPageAvailability"], "unavailable");
        assert_eq!(product["metadataUnavailable"], true);
        let deleted = product.clone();
        assert!(apply_public_response(&mut product, "900001", 503, None).is_err());
        assert_eq!(product, deleted);
        apply_public_response(
            &mut product,
            "900001",
            200,
            Some(&json!({"id":900001,"name":"Authored VRChat tool"})),
        )
        .unwrap();
        assert_eq!(product["publicPageAvailability"], "available");
        assert_eq!(product["metadataUnavailable"], false);
        assert_eq!(product["sourceDescription"], before["sourceDescription"]);
        assert_eq!(product["sourceTags"], before["sourceTags"]);
        assert_eq!(product["sourceCategory"], before["sourceCategory"]);
    }
}
#[test]
fn failed_or_malformed_public_response_never_claims_deleted_or_erases_metadata() {
    for status in [401, 403, 429, 500, 503] {
        let mut product = retained_product();
        let before = product.clone();
        assert!(apply_public_response(&mut product, "900001", status, None).is_err());
        assert_eq!(product, before);
    }
    for payload in [
        json!({}),
        json!({"name":"Invented"}),
        json!({"id":900001,"name":""}),
        json!({"id":900002,"name":"Wrong item"}),
        json!({"id":900001,"name":"<script>Invalid</script>"}),
    ] {
        let mut product = retained_product();
        let before = product.clone();
        assert!(apply_public_response(&mut product, "900001", 200, Some(&payload)).is_err());
        assert_eq!(product, before);
    }
    let mut new = json!({"id":"900001","description":"<p>Legacy Unity VRChat</p>"});
    ensure_source_fields(&mut new).unwrap();
    assert_eq!(new["publicPageAvailability"], "unknown");
    assert_eq!(new["sourceDescription"], "Legacy Unity VRChat");
    let mut sparse = retained_product();
    apply_public_response(
        &mut sparse,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Authored VRChat tool","description":"  ","category":{}})),
    )
    .unwrap();
    assert_eq!(
        sparse["sourceDescription"],
        "Retained VRChat Unity description"
    );
    assert_eq!(sparse["description"], "Retained VRChat Unity description");
    assert_eq!(sparse["sourceCategory"], json!({"name":"3D tools"}));
    sparse["sourceCategory"] = json!({"id":7,"name":"Shader","parentId":2,"parentName":"3D"});
    apply_public_response(
        &mut sparse,
        "900001",
        200,
        Some(&json!({"id":900001,"name":"Authored VRChat tool","category":{"id":7}})),
    )
    .unwrap();
    assert_eq!(
        sparse["sourceCategory"],
        json!({"id":7,"name":"Shader","parentId":2,"parentName":"3D"})
    );
    apply_public_response(&mut sparse, "900001", 200, Some(&json!({"id":900001,"name":"Authored VRChat tool","category":{"id":8,"name":"Motion","parent":{"id":3,"name":"Animation"}}}))).unwrap();
    assert_eq!(
        sparse["sourceCategory"],
        json!({"id":8,"name":"Motion","parentId":3,"parentName":"Animation"})
    );
}
/// Publication copies only `PUBLIC_FIELDS` from an enriched draft, so a field these writers
/// add without listing it would never reach the product.
#[test]
fn public_metadata_writes_only_fields_publication_copies() {
    let written = |before: &Value, after: &Value| -> Vec<String> {
        let (before, after) = (before.as_object().unwrap(), after.as_object().unwrap());
        before
            .keys()
            .chain(after.keys())
            .filter(|key| before.get(*key) != after.get(*key))
            .filter(|key| *key != "id" && !PUBLIC_FIELDS.contains(&key.as_str()))
            .cloned()
            .collect()
    };
    let payload = json!({"id":900001,"name":"Authored outfit","description":"Authored text",
        "published_at":"2026-01-02T03:04:05+09:00","price":"¥ 500","is_adult":false,
        "shop":{"name":"Authored shop","url":"https://authored.booth.pm/","uuid":"authored-uuid"},
        "category":{"id":208,"name":"3D衣装","parent":{"id":200,"name":"3Dモデル"}},
        "tags":[{"name":"VRChat"}],"images":[{"original":"https://booth.pximg.net/authored.png"}],
        "variations":[{"id":900010,"name":"Full","type":"digital","price":500}]});
    let fresh = public_metadata(&payload, "900001").unwrap();
    let keys: Vec<_> = fresh.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        written(&json!({}), &fresh),
        Vec::<String>::new(),
        "{keys:?}"
    );
    let stored =
        json!({"id":"900001","store":"BOOTH","acquired":"2026/01/01","categoryOverride":"Hair"});
    for status in [200, 404] {
        let mut product = stored.clone();
        apply_public_response(&mut product, "900001", status, Some(&payload)).unwrap();
        assert_eq!(written(&stored, &product), Vec::<String>::new(), "{status}");
    }
    let mut product = stored.clone();
    apply_public_support_html(
        &mut product,
        "<section><h2>対応アバター</h2><p>しなの https://ponderogen.booth.pm/items/6106863</p></section>",
    )
    .unwrap();
    assert!(product["publicSupportAvatars"].is_array());
    assert_eq!(written(&stored, &product), Vec::<String>::new());
}
