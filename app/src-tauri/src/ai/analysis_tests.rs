use super::*;
fn input() -> AnalysisInput {
    AnalysisInput {
        title: "Synthetic casual jacket".into(),
        source_tags: vec!["Clothes".into()],
        description: String::new(),
        rejected_tags: vec!["カジュアル".into()],
        files: vec![FileEvidence {
            evidence_ref: "file:1".into(),
            file_name: "docs/readme.txt".into(),
            text_excerpt: "Synthetic maker product reference".into(),
            candidate_public_urls: vec!["https://maker.gumroad.com/l/jacket".into()],
        }],
    }
}
fn result() -> Value {
    json!({"category":{"category":"Clothes","evidenceRefs":["title"],"basis":"Product type","confidence":0.9},
            "tags":[{"label":"Casual","evidenceRefs":["title"],"basis":"Style","confidence":0.8},{"label":"Coat","evidenceRefs":["file:1"],"basis":"Readme","confidence":0.7},{"label":"coat","evidenceRefs":["title"],"basis":"Name","confidence":0.7}],
            "tagAliases":[{"from":"Casual","to":"Everyday","evidenceRefs":["title"],"basis":"Style synonym","confidence":0.7}],
            "sourceUrlCandidates":[{"url":"https://maker.gumroad.com/l/jacket","evidenceRefs":["file:1"],"basis":"Readme reference; not verified","confidence":0.9}]})
}
#[test]
fn analysis_preserves_rejections_and_validates_evidence_without_claiming_ownership() {
    let parsed = parse_output(&result().to_string(), None, &input()).unwrap();
    assert_eq!(parsed.tags.len(), 1);
    assert_eq!(parsed.tags[0].label, "Coat");
    assert!(parsed.tag_aliases.is_empty());
    assert_eq!(parsed.category.unwrap().category, "Clothes");
    assert_eq!(parsed.source_url_candidates[0].evidence_refs, ["file:1"]);
    for (field, value) in [
        ("confidence", json!(1.1)),
        ("evidenceRefs", json!(["missing"])),
    ] {
        let mut output = result();
        output["tags"][1][field] = value;
        assert!(parse_output(&output.to_string(), None, &input()).is_err());
    }
    let mut output = result();
    output["sourceUrlCandidates"][0]["verified"] = json!(true);
    assert!(parse_output(&output.to_string(), None, &input()).is_err());
    let mut output = result();
    output.as_object_mut().unwrap().remove("category");
    assert!(parse_output(&output.to_string(), None, &input()).is_err());
    let mut output = result();
    output["category"] = Value::Null;
    assert!(parse_output(&output.to_string(), None, &input())
        .unwrap()
        .category
        .is_none());
    assert_eq!(
        parse_output("not JSON", Some("Provider refusal text"), &input())
            .err()
            .unwrap()
            .to_string(),
        "AI_REFUSED"
    );
}
#[test]
fn analysis_input_excludes_unknown_fields_absolute_paths_and_excess_content() {
    let mut value = serde_json::to_value(input()).unwrap();
    value["account"] = json!("forbidden");
    assert!(serde_json::from_value::<AnalysisInput>(value).is_err());
    for path in [
        "C:/private/readme.txt",
        "/private/readme.txt",
        "../readme.txt",
        "folder/../readme.txt",
        "\\\\server\\readme.txt",
    ] {
        let mut value = input();
        value.files[0].file_name = path.into();
        assert!(normalize_input(value).is_err());
    }
    let mut value = input();
    value.files[0].text_excerpt = "a".repeat(8193);
    assert!(normalize_input(value).is_err());
    let mut value = input();
    value.files[0].evidence_ref = "title".into();
    assert!(normalize_input(value).is_err());
    let mut value = input();
    value.files = vec![FileEvidence {
        evidence_ref: format!("i-{}", "a".repeat(64)),
        file_name: "image-1.png".into(),
        text_excerpt: String::new(),
        candidate_public_urls: vec![],
    }];
    assert!(normalize_input(value.clone()).is_ok());
    value.files[0]
        .candidate_public_urls
        .push("https://maker.gumroad.com/l/asset".into());
    assert!(normalize_input(value.clone()).is_err());
    value.files[0].candidate_public_urls.clear();
    value.files[0].evidence_ref = "i-not-a-content-hash".into();
    assert!(normalize_input(value).is_err());
}
#[test]
fn analysis_product_urls_accept_only_three_store_public_product_routes() {
    for url in [
        "https://booth.pm/items/123",
        "https://booth.pm/ja/items/123",
        "https://maker.booth.pm/en/items/123",
        "https://maker.gumroad.com/l/jacket",
        "https://jinxxy.com/maker/jacket",
        "https://ja.jinxxy.com/maker/jacket",
        "https://zh.jinxxy.com/maker/jacket",
    ] {
        assert!(public_product_url(url).is_some(), "{url}");
    }
    for url in [
        "https://booth.pm/items/0",
        "https://booth.pm/items/no",
        "https://booth.pm/downloadables/123",
        "https://booth.pm/items/123?token=secret",
        "https://booth.pm/items/123#fragment",
        "https://user:pass@booth.pm/items/123",
        "http://booth.pm/items/123",
        "https://booth.pm:444/items/123",
        "https://booth.pm.evil.test/items/123",
        "https://a.b.booth.pm/items/123",
        "https://gumroad.com/l/jacket",
        "https://checkout.gumroad.com/l/jacket",
        "https://maker.gumroad.com/checkout/jacket",
        "https://api.jinxxy.com/maker/jacket",
        "https://it.jinxxy.com/maker/jacket",
        "https://jinxxy.com/my/inventory",
        "https://jinxxy.com/downloads/file",
        "https://jinxxy.com/maker/%2e%2e",
        "https://jinxxy.com/maker/../jacket",
        "https://127.0.0.1/maker/jacket",
        "https://jinxxy.com/maker/jacket?signature=secret",
    ] {
        assert!(public_product_url(url).is_none(), "{url}");
    }
}
