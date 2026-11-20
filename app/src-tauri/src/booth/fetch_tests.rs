use super::*;
use serde_json::json;
#[test]
fn editor_url_fetch_accepts_only_canonical_public_booth_items() {
    for raw in [
        "https://booth.pm/items/900001",
        "https://booth.pm/ja/items/900001",
        "https://booth.pm/ko/items/900001",
        "https://booth.pm/zh-cn/items/900001",
        "https://booth.pm/zh-tw/items/900001",
        "https://synthetic.booth.pm/en/items/900001",
    ] {
        assert_eq!(item_url(raw).unwrap().id, "900001");
    }
    assert_eq!(
        item_url("https://booth.pm/items/900001")
            .unwrap()
            .source
            .as_str(),
        "https://booth.pm/ja/items/900001"
    );
    assert_eq!(
        item_url("https://synthetic.booth.pm/en/items/900001")
            .unwrap()
            .source
            .as_str(),
        "https://synthetic.booth.pm/en/items/900001"
    );
    for raw in [
        "http://booth.pm/items/900001",
        "https://user:secret@booth.pm/items/900001",
        "https://booth.pm:443/items/900001",
        "https://booth.pm/items/900001?token=PRIVATE_TOKEN",
        "https://booth.pm/items/900001#private",
        "https://booth.pm/ignored/../items/900001",
        "https://booth.pm/items/%39%30%30%30%30%31",
        "https://booth.pm/items/900001/",
        "https://booth.pm/downloadables/900001",
        "https://accounts.booth.pm/items/900001",
        "https://manage.booth.pm/items/900001",
        "https://booth.pm\\items\\900001",
        "https://boo\nth.pm/items/900001",
    ] {
        assert_eq!(
            item_url(raw).err().unwrap().to_string(),
            "INVALID_PRODUCT_URL"
        );
    }
    assert_eq!(
        item_url("https://gumroad.com/items/900001")
            .err()
            .unwrap()
            .to_string(),
        "UNSUPPORTED_PROVIDER"
    );
    assert_eq!(
        item_url("https://booth.pm.evil.test/items/900001")
            .err()
            .unwrap()
            .to_string(),
        "UNSUPPORTED_PROVIDER"
    );
}
#[test]
fn fetched_editor_values_have_closed_schema_and_never_include_entitlements() {
    let payload = json!({"id":900001,"name":"<b>Authored VRChat Shader</b>","description":"Unity shader for VRChat","category":{"name":"3D"},"tags":[{"name":"VRChat"},{"name":"Shader"}],"shop":{"url":"https://synthetic.booth.pm/"},"images":[{"original":"https://booth.pximg.net/unsafe.png?token=PRIVATE_TOKEN"},{"original":"https://booth.pximg.net/authored.png"}],"cookies":"PRIVATE_SESSION","variations":[{"downloadable":{"url":"https://booth.pm/downloadables/700001?signature=PRIVATE_SIGNATURE"}}]});
    let value = serde_json::to_value(
        staged(
            &item_url("https://synthetic.booth.pm/items/900001").unwrap(),
            &payload,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(value["title"], "Authored VRChat Shader");
    assert_eq!(value["category"], "Shader");
    assert_eq!(value["tags"], json!(["VRChat", "Shader"]));
    assert_eq!(
        value["thumbnailUrl"],
        "https://booth.pximg.net/authored.png"
    );
    assert_eq!(
        value["sourceUrl"],
        "https://synthetic.booth.pm/items/900001"
    );
    let keys = ["sourceUrl", "title", "category", "tags", "thumbnailUrl"];
    assert_eq!(value.as_object().unwrap().len(), keys.len());
    assert!(value
        .as_object()
        .unwrap()
        .keys()
        .all(|key| keys.contains(&key.as_str())));
    assert!(!value.to_string().contains("PRIVATE"));
    let mut no_image = payload.clone();
    no_image["images"] = json!([]);
    assert!(serde_json::to_value(
        staged(
            &item_url("https://booth.pm/items/900001").unwrap(),
            &no_image
        )
        .unwrap()
    )
    .unwrap()["thumbnailUrl"]
        .is_null());
    assert_eq!(
        staged(
            &item_url("https://other.booth.pm/items/900001").unwrap(),
            &payload
        )
        .err()
        .unwrap()
        .to_string(),
        "PUBLIC_SOURCE_HOST_MISMATCH"
    );
    assert_eq!(
        staged(
            &item_url("https://booth.pm/items/900002").unwrap(),
            &payload
        )
        .err()
        .unwrap()
        .to_string(),
        "PUBLIC_ID_MISMATCH"
    );
}
#[test]
fn local_price_is_the_cheapest_digital_variation_or_the_listing_headline() {
    let item = item_url("https://booth.pm/items/900001").unwrap();
    let price = |listing: Value, variations: Value| {
        let payload =
            json!({"id":900001,"name":"Authored","price":listing,"variations":variations});
        serde_json::to_value(staged(&item, &payload).unwrap()).unwrap()["listingPrice"].clone()
    };
    assert_eq!(
        price(
            json!("¥ 300 ~"),
            json!([
                {"id":800001,"name":"Full","type":"digital","price":4200},
                {"id":800002,"name":"One avatar","type":"digital","price":1800},
                {"id":800003,"name":"Printed","type":"direct","price":300}
            ])
        ),
        json!({"price":1800.0,"priceCurrency":"JPY"})
    );
    assert_eq!(
        price(
            json!("¥ 1,500 ~"),
            json!([{"id":800001,"type":"direct","price":1500}])
        ),
        json!({"price":1500.0,"priceCurrency":"JPY"})
    );
    assert_eq!(
        price(json!(0), json!([{"id":800001,"type":"digital","price":0}])),
        json!({"price":0.0,"priceCurrency":"JPY"})
    );
    assert!(price(Value::Null, json!([])).is_null());
}
