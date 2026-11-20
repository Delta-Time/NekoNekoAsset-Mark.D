
use super::*;

fn authored_product() -> Value {
    json!({
        "__typename":"PublicProduct", "id":"42", "url":"product",
        "user":{"username":"authored"}, "name":"Authored Coat",
        "category":"avatar-clothing", "type":["clothing"], "tags":["coat","cloth"],
        "cover":{"url":"https://booth.pximg.net/authored.png"},
        "description":"$a", "product_type":"DIGITAL_PRODUCT",
        "files":[{"secret":"NOT_PRESENTATION"}], "license":"NOT_PRESENTATION"
    })
}

#[test]
fn gumroad_url_fetch_requires_explicit_title_or_visible_product_body_vrchat_evidence() {
    let source = Url::parse("https://authored.gumroad.com/l/shader").unwrap();
    let mut product = json!({"api_version":1,"id":"authored-product",
            "short_permalink":"shader","name":"Authored Shader",
            "tags":["Shader","VRChat"],"description":"Blender shader"});
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "NonVRChat"
    );
    product["description"] = json!("<p>Shader for VR<strong>Chat</strong></p>");
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "Shader"
    );
    product["description"] = json!("<a href='https://example.invalid/VRChat' title='VRChat'>Read manual</a><img alt='VRC'><script>VRChat</script><style>VRC</style><nav>VRChat</nav>");
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "NonVRChat"
    );
    product["name"] = json!("Authored VRC Shader");
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "Shader"
    );
    product["name"] = json!("Authored Shader");
    product["description"] = json!("Shader for &lt;VRChat&gt;");
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "Shader"
    );
    product["description"] = Value::Null;
    assert_eq!(
        gumroad_values(&source, &product).unwrap()["category"],
        "NonVRChat"
    );
}

fn authored_flight(product: Value, description: Option<&str>) -> String {
    let mut stream = format!("1:{}\n", json!({"product":product}));
    if let Some(description) = description {
        stream.push_str(&format!("a:T{:x},{description}\n", description.len()));
    }
    let split = stream
        .char_indices()
        .map(|(offset, _)| offset)
        .find(|offset| *offset >= stream.len() / 2)
        .unwrap_or(stream.len());
    format!(
            "<script>(self.__next_f=self.__next_f||[]).push([0]);self.__next_f.push({});</script><script>self.__next_f.push({})</script>",
            json!([1,&stream[..split]]), json!([1,&stream[split..]])
        )
}

#[test]
fn jinxxy_public_flight_keeps_only_bound_presentation() {
    let description = json!({"root":{"type":"root","children":[
            {"type":"heading","children":[{"type":"text","text":"Authored coat"}]},
            {"type":"paragraph","children":[
                {"type":"text","text":"For VRChat. ","style":"NON_TEXT".repeat(3000)},
                {"type":"link","url":"https://example.com/NON_TEXT","children":[{"type":"text","text":"Read manual"}]},
                {"type":"linebreak"}, {"type":"text","text":"対応衣装"}
            ]},
            {"type":"media","url":"https://example.com/NON_TEXT","text":"NON_TEXT"}
        ]}}).to_string();
    assert!(description.len() > 16384);
    let source = Url::parse("https://jinxxy.com/authored/product").unwrap();
    let html = authored_flight(authored_product(), Some(&description));
    let presentation = jinxxy_presentation(&source, &Html::parse_document(&html), Some("42"))
        .unwrap()
        .unwrap();
    assert_eq!(presentation["publicProductId"], "42");
    assert_eq!(presentation["sourceUrl"], source.as_str());
    assert_eq!(presentation["sourceTitle"], "Authored Coat");
    assert_eq!(
        presentation["sourceDescription"],
        "Authored coat For VRChat. Read manual 対応衣装"
    );
    assert_eq!(
        presentation["sourceCategory"],
        json!({"name":"avatar-clothing","types":["clothing"]})
    );
    assert_eq!(presentation["sourceTags"], json!(["coat", "cloth"]));
    assert_eq!(presentation.as_object().unwrap().len(), 7);
    assert!(!presentation.to_string().contains("NOT_PRESENTATION"));
    assert!(!presentation.to_string().contains("NON_TEXT"));
    assert!(jinxxy_presentation(&source, &Html::parse_document(&html), Some("43")).is_err());
    let other = Url::parse("https://jinxxy.com/other/product").unwrap();
    assert!(jinxxy_presentation(&other, &Html::parse_document(&html), Some("42")).is_err());
    let other = Url::parse("https://jinxxy.com/authored/other").unwrap();
    assert!(jinxxy_presentation(&other, &Html::parse_document(&html), Some("42")).is_err());
    let editor = html_values(&source, &html).unwrap();
    assert_eq!(editor["title"], "Authored Coat");
    assert_eq!(editor["tags"], json!(["cloth", "coat"]));
    assert_eq!(editor.as_object().unwrap().len(), 5);
}

#[test]
fn local_price_comes_from_the_public_listing_and_a_copied_link_still_works() {
    let source = Url::parse("https://jinxxy.com/authored/product").unwrap();
    let mut product = authored_product();
    product["base_price"] = json!(5000);
    product["currency_code"] = json!("USD");
    let editor = html_values(&source, &authored_flight(product.clone(), None)).unwrap();
    assert_eq!(
        editor["listingPrice"],
        json!({"price":50.0,"priceCurrency":"USD"})
    );
    product["currency_code"] = json!("EUR");
    let editor = html_values(&source, &authored_flight(product, None)).unwrap();
    assert!(editor.get("listingPrice").is_none());

    let source = Url::parse("https://authored.gumroad.com/l/shader").unwrap();
    let public = |extra: Value| {
        let mut value = json!({"api_version":1,"id":"authored-id","short_permalink":"shader",
                "name":"Authored Shader","is_pay_what_you_want":false,"is_recurring_billing":false});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        gumroad_values(&source, &value).unwrap()
    };
    assert_eq!(
        public(json!({"price_cents":2000,"currency_code":"usd","price_formatted":"$20"}))
            ["listingPrice"],
        json!({"price":20.0,"priceCurrency":"USD","priceFormatted":"$20","priceSourceCurrency":"USD"})
    );
    assert_eq!(
        public(json!({"price_cents":1000,"currency_code":"jpy","price_formatted":"¥1,000"}))
            ["listingPrice"],
        json!({"price":null,"priceCurrency":null,"priceFormatted":"¥1,000","priceSourceCurrency":"JPY"})
    );
    assert!(public(json!({})).get("listingPrice").is_none());

    for (raw, bare) in [
        (
            "https://author.gumroad.com/l/shader?layout=profile",
            "https://author.gumroad.com/l/shader",
        ),
        (
            "https://booth.pm/zh-tw/items/900001/#top",
            "https://booth.pm/zh-tw/items/900001",
        ),
        (
            " https://jinxxy.com/authored/product/?ref=1 ",
            "https://jinxxy.com/authored/product",
        ),
    ] {
        assert_eq!(bare_url(raw).unwrap(), bare);
    }
    // Only the three stores are ever read.
    assert_eq!(
        product_url(&bare_url("https://example.com/l/shader").unwrap())
            .unwrap_err()
            .to_string(),
        "UNSUPPORTED_PROVIDER"
    );
}

#[test]
fn jinxxy_public_optional_fields_do_not_invent_metadata() {
    let source = Url::parse("https://zh.jinxxy.com/authored/product").unwrap();
    let product = json!({"__typename":"PublicProduct","id":"42","url":"product","user":{"username":"authored"}});
    let html = authored_flight(product, None);
    let presentation = jinxxy_presentation(&source, &Html::parse_document(&html), Some("42"))
        .unwrap()
        .unwrap();
    assert_eq!(
        presentation,
        json!({"publicProductId":"42","sourceUrl":source.as_str()})
    );
    let html = format!("<meta property=\"og:title\" content=\"Authored title\">{html}");
    let editor = html_values(&source, &html).unwrap();
    assert_eq!(editor["title"], "Authored title");
    assert_eq!(editor["tags"], json!([]));
    assert_eq!(editor.as_object().unwrap().len(), 5);
    assert!(jinxxy_presentation(
        &source,
        &Html::parse_document("<p>No product data</p>"),
        Some("42")
    )
    .unwrap()
    .is_none());
}

#[test]
fn jinxxy_flight_text_lengths_are_utf8_bytes_and_scripts_are_data() {
    let source = Url::parse("https://jinxxy.com/authored/product").unwrap();
    let html = authored_flight(authored_product(), Some("衣装\nsecond line"));
    let presentation = jinxxy_presentation(&source, &Html::parse_document(&html), Some("42"))
        .unwrap()
        .unwrap();
    assert_eq!(presentation["sourceDescription"], "衣装 second line");
    let invalid = format!(
        "<script>self.__next_f.push({})</script>",
        json!([1, "a:T1,衣"])
    );
    assert!(jinxxy_flight(&Html::parse_document(&invalid)).is_err());
    let invalid = format!(
        "<script>self.__next_f.push({})</script>",
        json!([1, "a:Tff,short"])
    );
    assert!(jinxxy_flight(&Html::parse_document(&invalid)).is_err());
    assert!(jinxxy_flight(&Html::parse_document(
        "<script>self.__next_f.push(runCode())</script>"
    ))
    .is_err());
}

#[test]
fn jinxxy_flight_preserves_raw_script_text_record_bytes() {
    let description = json!({"root":{"type":"root","children":[
            {"type":"paragraph","children":[{"type":"text","text":"Raw <angle> & nonbreaking\u{00a0}space"}]}
        ]}}).to_string();
    let html = authored_flight(authored_product(), Some(&description));
    let document = Html::parse_document(&html);
    assert!(document
        .select(&Selector::parse("script").unwrap())
        .any(|script| { script.inner_html() != script.text().collect::<String>() }));
    let records = jinxxy_flight(&document).unwrap();
    assert_eq!(records["a"], description);
    let source = Url::parse("https://jinxxy.com/authored/product").unwrap();
    let presentation = jinxxy_presentation(&source, &document, Some("42"))
        .unwrap()
        .unwrap();
    assert_eq!(
        presentation["sourceDescription"],
        "Raw <angle> & nonbreaking space"
    );
    assert_eq!(
        html_values(&source, &html)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn jinxxy_flight_skips_idless_preload_hints_but_requires_model_ids() {
    let html = format!("<script>self.__next_f.push({})</script>",
            json!([1,":HL[\"/authored.css\",\"style\"]\n:HL[\"/authored.woff2\",\"font\"]\n1:{\"name\":\"Authored model\"}\n"]));
    let records = jinxxy_flight(&Html::parse_document(&html)).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records["1"]["name"], "Authored model");
    let invalid = format!(
        "<script>self.__next_f.push({})</script>",
        json!([1, ":{\"name\":\"Missing model ID\"}\n"])
    );
    assert!(jinxxy_flight(&Html::parse_document(&invalid)).is_err());
}

#[test]
fn jinxxy_description_ignores_unrecognized_json_fields() {
    assert_eq!(
        jinxxy_description(r#"{"root":{"children":[],"secret":"IGNORE"}}"#),
        ""
    );
    assert_eq!(jinxxy_description(r#"{"root":{"secret":"IGNORE"}}"#), "");
    assert_eq!(jinxxy_description("<p>Authored coat</p>"), "Authored coat");
}

#[test]
fn public_editor_urls_and_results_do_not_grant_ownership() {
    for raw in [
        "https://author.gumroad.com/l/authored",
        "https://gumroad.com/l/authored",
        "https://jinxxy.com/authored/product",
    ] {
        assert!(product_url(raw).unwrap().is_some());
    }
    for raw in [
        "https://jinxxy.com/my/inventory",
        "https://jinxxy.com/a/../b/c",
        "https://jinxxy.com/a/b?secret=1",
        "https://gumroad.com/d/secret",
        "https://files.gumroad.com/l/secret",
        "https://secret@jinxxy.com/a/b",
        "https://jinxxy.com:443/a/b",
        "https://jinxxy.com/a/%62",
    ] {
        assert!(product_url(raw).is_err(), "{raw}");
    }
    let source = Url::parse("https://jinxxy.com/authored/product").unwrap();
    let value = html_values(&source,r#"<meta property="og:title" content="Authored Shader"><meta property="og:description" content="VRChat Unity shader"><meta name="keywords" content="VRChat,Shader"><meta property="og:image" content="https://booth.pximg.net/authored.png"><script>PRIVATE_INVENTORY</script>"#).unwrap();
    assert_eq!(value["category"], "Shader");
    assert_eq!(value.as_object().unwrap().len(), 5);
    assert!(!value.to_string().contains("PRIVATE"));
    assert!(html_values(&source,r#"<meta property="og:title" content="Other"><meta property="og:url" content="https://jinxxy.com/other/product">"#).is_err());
    let source = Url::parse("https://authored.gumroad.com/l/shader").unwrap();
    let value = gumroad_values(&source,&json!({"api_version":1,"id":"authored-id","short_permalink":"shader","name":"Authored Shader","description":"VRChat shader","tags":["Shader"],"thumbnail_url":"https://booth.pximg.net/a.png","purchase":"PRIVATE"})).unwrap();
    assert_eq!(value["title"], "Authored Shader");
    assert_eq!(value.as_object().unwrap().len(), 5);
}
