
use super::*;

#[test]
fn image_analysis_tracks_preview_identity_and_chat_candidates_exclude_private_fields() {
    let dir = std::env::temp_dir().join(format!("nna-ai-preview-synthetic-{}", Uuid::new_v4()));
    let c = Catalogue::open(&dir).unwrap();
    let id = "local:synthetic-preview";
    let mut source = json!({"id":id,"provider":"local","store":"Local Files","custom":true,"title":"Authored purple jacket","sourceTags":["Purple"],"previewUrl":"https://example.invalid/one.png","privateEvidence":"DO_NOT_SEND","sourceCategory":{"name":"Hair"}});
    c.put("product", id, source.clone()).unwrap();
    c.command(
        "overlay",
        json!({"ids":[id],"patch":{"note":"DO_NOT_SEND"}}),
    )
    .unwrap();
    let mut snapshot = c.collect_ai_metadata(id).unwrap();
    let reference = format!("i-{}", "a".repeat(64));
    snapshot
        .input
        .files
        .push(crate::ai::analysis::FileEvidence {
            evidence_ref: reference.clone(),
            file_name: "image-1.png".into(),
            text_excerpt: String::new(),
            candidate_public_urls: vec![],
        });
    let result=serde_json::from_value(json!({"category":{"category":"Clothes","evidenceRefs":[reference],"basis":"Visual garment evidence","confidence":0.8},"tags":[],"tagAliases":[],"sourceUrlCandidates":[]})).unwrap();
    assert!(c
        .publish_ai_analysis(
            id,
            &snapshot.catalogue_fingerprint,
            &snapshot.input,
            &result,
            "synthetic-model"
        )
        .is_err());
    c.publish_ai_analysis_with_preview(
        id,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &result,
        "synthetic-model",
        Some(&snapshot.preview_fingerprint),
    )
    .unwrap();
    assert_eq!(
        c.command("product_details", json!({"id":id})).unwrap()["aiCurrent"],
        true
    );
    let candidates = c
        .ai_recommendation_candidates("purple jacket", None, None, &[])
        .unwrap();
    assert_eq!(candidates[0]["productId"], id);
    assert!(!serde_json::to_string(&candidates)
        .unwrap()
        .contains("DO_NOT_SEND"));
    assert!(candidates[0].get("accountIds").is_none());
    assert!(candidates[0].get("previewUrl").is_none());
    source["previewUrl"] = json!("https://example.invalid/two.png");
    c.put("product", id, source).unwrap();
    assert_eq!(
        c.command("product_details", json!({"id":id})).unwrap()["aiCurrent"],
        false
    );
    assert_eq!(
        c.publish_ai_analysis_with_preview(
            id,
            &snapshot.catalogue_fingerprint,
            &snapshot.input,
            &result,
            "synthetic-model",
            Some(&snapshot.preview_fingerprint)
        )
        .unwrap_err()
        .to_string(),
        "AI_INPUT_STALE"
    );
    snapshot.input.files.clear();
    let text_result=serde_json::from_value(json!({"category":{"category":"Clothes","evidenceRefs":["title"],"basis":"Title evidence","confidence":0.8},"tags":[],"tagAliases":[],"sourceUrlCandidates":[]})).unwrap();
    c.publish_ai_analysis(
        id,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &text_result,
        "synthetic-model",
    )
    .unwrap();
    assert_eq!(
        c.command("product_details", json!({"id":id})).unwrap()["aiCurrent"],
        true
    );
    let mut preview_product = c.get("product", id).unwrap().unwrap();
    preview_product["previewUrl"] = json!("https://i.pximg.net/authored-source.png");
    preview_product["thumbnailPath"] = json!("media/source-only.png");
    c.put("product", id, preview_product.clone()).unwrap();
    c.put("overlay", id, json!({"id":id})).unwrap();
    let handles = c.ai_preview_source(id).unwrap();
    assert_eq!(
        handles.preview_fingerprint,
        c.collect_ai_metadata(id).unwrap().preview_fingerprint
    );
    assert_eq!(
        handles.public_url.as_deref(),
        Some("https://i.pximg.net/authored-source.png")
    );
    assert!(handles.managed_relative_path.is_none());
    c.put(
        "overlay",
        id,
        json!({"id":id,"sourceThumbnailUrl":"https://i.pximg.net/authored-override.png"}),
    )
    .unwrap();
    assert_eq!(
        c.ai_preview_source(id).unwrap().public_url.as_deref(),
        Some("https://i.pximg.net/authored-override.png")
    );
    c.put("overlay", id, json!({"id":id,"sourceThumbnailUrl":"https://i.pximg.net/authored-override.png","thumbnailPath":"media\\authored.png"})).unwrap();
    let handles = c.ai_preview_source(id).unwrap();
    assert_eq!(
        handles.managed_relative_path.as_deref(),
        Some("media/authored.png")
    );
    assert!(handles.public_url.is_none());
    c.put(
        "overlay",
        id,
        json!({"id":id,"thumbnailPath":"media/authored.png","thumbnailHidden":true}),
    )
    .unwrap();
    let handles = c.ai_preview_source(id).unwrap();
    assert!(handles.managed_relative_path.is_none() && handles.public_url.is_none());
    assert_eq!(
        handles.preview_fingerprint,
        c.collect_ai_metadata(id).unwrap().preview_fingerprint
    );
    c.put("overlay", id, json!({"id":id,"sourceThumbnailUrl":""}))
        .unwrap();
    assert!(c.ai_preview_source(id).unwrap().public_url.is_none());
    c.put(
        "overlay",
        id,
        json!({"id":id,"thumbnailPath":"other/authored.png"}),
    )
    .unwrap();
    assert!(c.ai_preview_source(id).is_err());
    c.put("overlay", id, json!({"id":id})).unwrap();
    preview_product["previewUrl"] = Value::Null;
    c.put("product", id, preview_product).unwrap();
    let handles = c.ai_preview_source(id).unwrap();
    assert!(handles.managed_relative_path.is_none() && handles.public_url.is_none());
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ai_metadata_result_survives_download_but_publication_still_rejects_stale_snapshot() {
    let dir = std::env::temp_dir().join(format!("nna-ai-evidence-synthetic-{}", Uuid::new_v4()));
    let c = Catalogue::open(&dir).unwrap();
    let id = "local:metadata-ai";
    c.put("product", id, json!({"id":id,"provider":"local","store":"Local Files","custom":true,"title":"Synthetic garment","sourceTags":["Source Tag"],"sourceCategory":{"name":"Hair"}})).unwrap();
    c.put("remote_file", "synthetic-file", json!({"id":"synthetic-file","productId":id,"provider":"local","name":"Assets/model.zip","revision":"one"})).unwrap();
    let mut snapshot = c.collect_ai_metadata(id).unwrap();
    snapshot.input.files.clear();
    let result = serde_json::from_value(json!({"category":{"category":"Clothes","evidenceRefs":["title"],"basis":"Authored title","confidence":0.9},"tags":[],"tagAliases":[],"sourceUrlCandidates":[]})).unwrap();
    c.publish_ai_analysis(
        id,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    let aliases = vec![crate::ai::analysis::TagAliasSuggestion {
        from: "Source Tag".into(),
        to: "Organized Tag".into(),
        evidence_refs: vec!["sourceTags".into()],
        basis: "Explicit organization".into(),
        confidence: 0.9,
    }];
    c.publish_ai_tag_organization(
        id,
        &snapshot.catalogue_fingerprint,
        &aliases,
        "synthetic-model",
    )
    .unwrap();
    let artifact = json!({"id":"synthetic-artifact","productId":id,"fileId":"synthetic-file","revision":"one","hash":"downloaded-hash","relativePath":"managed/model.zip","status":"ready","extractedInventory":[{"relativePath":"README.md","hash":"readme-hash","sizeBytes":3,"isDirectory":false}]});
    c.put("artifact", "synthetic-artifact", artifact).unwrap();
    let details = c.command("product_details", json!({"id":id})).unwrap();
    assert_eq!(details["aiCurrent"], true);
    assert_eq!(details["category"], "Clothes");
    assert!(strings(&details["tags"]).contains(&"Organized Tag".into()));
    assert_eq!(
        c.publish_ai_analysis(
            id,
            &snapshot.catalogue_fingerprint,
            &snapshot.input,
            &result,
            "synthetic-model"
        )
        .unwrap_err()
        .to_string(),
        "AI_INPUT_STALE"
    );
    // Filename-only evidence likewise does not claim to have read file bytes.
    let mut names = c.collect_ai_metadata(id).unwrap();
    names
        .input
        .files
        .retain(|file| file.evidence_ref == opaque_ai_file_ref("synthetic-file", ""));
    c.publish_ai_analysis(
        id,
        &names.catalogue_fingerprint,
        &names.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    let mut artifact = c.get("artifact", "synthetic-artifact").unwrap().unwrap();
    artifact["hash"] = json!("changed-unread-bytes");
    c.put("artifact", "synthetic-artifact", artifact).unwrap();
    assert_eq!(
        c.command("product_details", json!({"id":id})).unwrap()["aiCurrent"],
        true
    );
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ai_catalogue_candidates_match_filename_but_never_establish_local_origin() {
    let dir = std::env::temp_dir().join(format!("nna-ai-candidates-synthetic-{}", Uuid::new_v4()));
    let c = Catalogue::open(&dir).unwrap();
    let local = "local:candidate-synthetic";
    c.put("product",local,json!({"id":local,"provider":"local","store":"Local Files","custom":true,"title":"Imported product"})).unwrap();
    c.put("remote_file","synthetic-zip",json!({"id":"synthetic-zip","productId":local,"provider":"local","name":"Futura_Meadow_v2.zip","revision":"authored"})).unwrap();
    for (id, provider, store, title, url) in [
        (
            "gumroad:futura",
            "gumroad",
            "Gumroad",
            "Futura Meadow VRChat Outfit",
            "https://maker.gumroad.com/l/futura-meadow",
        ),
        (
            "jinxxy:other",
            "jinxxy",
            "Jinxxy",
            "Unrelated Urban Clothes Version 2",
            "https://jinxxy.com/maker/urban",
        ),
        (
            "local:other",
            "local",
            "Local Files",
            "Futura Meadow",
            "https://maker.gumroad.com/l/local-copy",
        ),
        (
            "gumroad:bad",
            "gumroad",
            "Gumroad",
            "Futura Meadow",
            "https://unproved.example/l/futura",
        ),
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"provider":provider,"store":store,"sourceTitle":title,"sourceUrl":url}),
        )
        .unwrap();
    }
    let mut snapshot = c.collect_ai_metadata(local).unwrap();
    let candidates = c.ai_source_candidates(&snapshot.input).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].evidence_ref.starts_with("c-"));
    assert_eq!(
        candidates[0].candidate_public_urls,
        vec!["https://maker.gumroad.com/l/futura-meadow"]
    );
    let result:crate::ai::analysis::AnalysisOutput=serde_json::from_value(json!({"category":null,"tags":[],"tagAliases":[],"sourceUrlCandidates":[{"url":candidates[0].candidate_public_urls[0],"evidenceRefs":[candidates[0].evidence_ref],"basis":"Filename matches an existing public title; local origin unverified","confidence":0.99}]})).unwrap();
    snapshot.input.files.extend(candidates);
    c.publish_ai_analysis(
        local,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    let p = c.command("product_details", json!({"id":local})).unwrap();
    assert_eq!(p["aiSourceUrlCandidates"][0]["status"], "uncertain");
    assert_eq!(p["provider"], "local");
    assert!(p["sourceUrl"].is_null());
    assert!(c.related("entitlement", local).unwrap().is_empty());
    let mut unrelated = snapshot.input;
    unrelated.title = "VRChat Outfit Version 2".into();
    unrelated.files = vec![crate::ai::analysis::FileEvidence {
        evidence_ref: "e-generic".into(),
        file_name: "Unity_Avatar_v2.unitypackage".into(),
        text_excerpt: String::new(),
        candidate_public_urls: vec![],
    }];
    assert!(c.ai_source_candidates(&unrelated).unwrap().is_empty());
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ai_overlay_preserves_manual_edits_rejections_and_invalidates_changed_evidence() {
    let dir = std::env::temp_dir().join(format!("nna-ai-catalogue-synthetic-{}", Uuid::new_v4()));
    let c = Catalogue::open(&dir).unwrap();
    let id = "local:synthetic-ai";
    let mut source = json!({"id":id,"provider":"local","store":"Local Files","custom":true,"title":"Synthetic Hair","sourceTags":["Source Tag"],"sourceCategory":{"name":"Hair"},"sourceDescription":"Authored description"});
    c.put("product", id, source.clone()).unwrap();
    let records=(0..140).map(|n| {
            let file=format!("synthetic-file-{n:03}");
            ("remote_file".into(),file.clone(),json!({"id":file,"productId":id,"provider":"local","name":format!("Assets/{n:03}.txt"),"revision":"one"}))
        }).collect();
    c.put_many(records).unwrap();
    let mut artifact = json!({"id":"synthetic-artifact","productId":id,"fileId":"synthetic-file-000","revision":"one","hash":"hash-one","relativePath":"managed/000.txt","status":"ready","extractedInventory":[{"relativePath":"docs/readme.txt","hash":"readme-hash","sizeBytes":3,"isDirectory":false}]});
    c.put("artifact", "synthetic-artifact", artifact.clone())
        .unwrap();
    c.command("overlay",json!({"ids":[id],"patch":{"title":"Human Hair","note":"Human note","rejectedTags":["カジュアル"]}})).unwrap();
    let mut snapshot = c.collect_ai_metadata(id).unwrap();
    assert_eq!(snapshot.input.files.len(), 64);
    assert_eq!(snapshot.input.title, "Human Hair");
    assert!(snapshot
        .input
        .files
        .iter()
        .any(|f| f.file_name == "docs/readme.txt"));
    assert!(snapshot
        .input
        .files
        .iter()
        .all(|f| f.evidence_ref.starts_with("e-") && f.text_excerpt.is_empty()));
    assert!(serde_json::to_vec(&snapshot.input).unwrap().len() <= 128 * 1024);
    assert_eq!(
        snapshot.catalogue_fingerprint,
        c.collect_ai_metadata(id).unwrap().catalogue_fingerprint
    );
    let url = "https://maker.gumroad.com/l/synthetic";
    assert_eq!(snapshot.input.files[0].file_name, "docs/readme.txt");
    snapshot.input.files[0].text_excerpt = format!("Product reference: {url}");
    snapshot.input.files[0]
        .candidate_public_urls
        .push(url.into());
    let result: crate::ai::analysis::AnalysisOutput=serde_json::from_value(json!({
            "category":{"category":"Clothes","evidenceRefs":["title"],"basis":"Authored garment evidence","confidence":0.8},
            "tags":[{"label":"AI Tag","evidenceRefs":["title"],"basis":"Authored style","confidence":0.8},{"label":"Casual","evidenceRefs":["title"],"basis":"Rejected style","confidence":0.8}],
            "tagAliases":[],"sourceUrlCandidates":[{"url":url,"evidenceRefs":[snapshot.input.files[0].evidence_ref],"basis":"Embedded reference only","confidence":0.7}]
        })).unwrap();
    c.publish_ai_analysis(
        id,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    let details = |c: &Catalogue| c.command("product_details", json!({"id":id})).unwrap();
    let p = details(&c);
    assert_eq!(p["category"], "Clothes");
    assert_eq!(p["sourceCategory"]["name"], "Hair");
    assert!(strings(&p["tags"]).contains(&"Source Tag".into()));
    assert!(strings(&p["tags"]).contains(&"AI Tag".into()));
    assert!(!strings(&p["tags"]).contains(&"Casual".into()));
    assert_eq!(p["note"], "Human note");
    assert_eq!(p["provider"], "local");
    assert!(p["sourceUrl"].is_null());
    assert!(strings(&p["accountIds"]).is_empty());
    assert_eq!(p["aiSourceUrlCandidates"][0]["status"], "supported");
    let overlay = c.get("overlay", id).unwrap().unwrap();
    assert!(!overlay["ai"].to_string().contains("textExcerpt"));
    c.dismiss_ai_source_url_candidate(id, url).unwrap();
    c.publish_ai_analysis(
        id,
        &snapshot.catalogue_fingerprint,
        &snapshot.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    assert!(details(&c)["aiSourceUrlCandidates"]
        .as_array()
        .unwrap()
        .is_empty());
    artifact["status"] = json!("verifying");
    c.put("artifact", "synthetic-artifact", artifact.clone())
        .unwrap();
    assert_eq!(details(&c)["category"], "Clothes");
    artifact["hash"] = json!("hash-two");
    artifact["extractedInventory"][0]["hash"] = json!("changed-readme-hash");
    c.put("artifact", "synthetic-artifact", artifact.clone())
        .unwrap();
    assert_eq!(details(&c)["aiCurrent"], false);
    assert_eq!(details(&c)["category"], "Hair");
    assert_eq!(
        c.publish_ai_analysis(
            id,
            &snapshot.catalogue_fingerprint,
            &snapshot.input,
            &result,
            "synthetic-model"
        )
        .unwrap_err()
        .to_string(),
        "AI_INPUT_STALE"
    );
    let mut fresh = c.collect_ai_metadata(id).unwrap();
    fresh.input.files[0].candidate_public_urls.push(url.into());
    c.publish_ai_analysis(
        id,
        &fresh.catalogue_fingerprint,
        &fresh.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    c.command(
        "overlay",
        json!({"ids":[id],"patch":{"tags":["Source Tag"],"category":"Tool"}}),
    )
    .unwrap();
    assert!(
        strings(&c.get("overlay", id).unwrap().unwrap()["rejectedTags"]).contains(&"AI Tag".into())
    );
    let fresh = c.collect_ai_metadata(id).unwrap();
    c.publish_ai_analysis(
        id,
        &fresh.catalogue_fingerprint,
        &fresh.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    assert_eq!(details(&c)["category"], "Tool");
    assert_eq!(details(&c)["tags"], json!(["Source Tag"]));
    c.command(
        "overlay",
        json!({"ids":[id],"patch":{},"clearFields":["tags","category"]}),
    )
    .unwrap();
    let fresh = c.collect_ai_metadata(id).unwrap();
    c.publish_ai_analysis(
        id,
        &fresh.catalogue_fingerprint,
        &fresh.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    assert!(!strings(&details(&c)["tags"]).contains(&"AI Tag".into()));
    assert_eq!(details(&c)["category"], "Clothes");
    source["price"] = json!(7.0);
    c.put("product", id, source.clone()).unwrap();
    assert_eq!(details(&c)["aiCurrent"], true);
    let stale = c.collect_ai_metadata(id).unwrap();
    c.command(
        "overlay",
        json!({"ids":[id],"patch":{"title":"New human title"}}),
    )
    .unwrap();
    assert_eq!(
        c.publish_ai_analysis(
            id,
            &stale.catalogue_fingerprint,
            &stale.input,
            &result,
            "synthetic-model"
        )
        .unwrap_err()
        .to_string(),
        "AI_INPUT_STALE"
    );
    assert_eq!(details(&c)["title"], "New human title");
    assert!(c
        .command("overlay", json!({"ids":[id],"patch":{"ai":{}}}))
        .is_err());
    let fresh = c.collect_ai_metadata(id).unwrap();
    c.publish_ai_analysis(
        id,
        &fresh.catalogue_fingerprint,
        &fresh.input,
        &result,
        "synthetic-model",
    )
    .unwrap();
    assert_eq!(c.product_ids().unwrap(), [id]);
    c.prepare_sync_document().unwrap();
    drop(c);
    let c = Catalogue::open(&dir).unwrap();
    assert_eq!(details(&c)["aiCurrent"], true);
    assert_eq!(details(&c)["category"], "Clothes");
    assert!(details(&c)["aiSourceUrlCandidates"]
        .as_array()
        .unwrap()
        .is_empty());
    let organized = c.collect_ai_metadata(id).unwrap();
    let aliases = vec![
        crate::ai::analysis::TagAliasSuggestion {
            from: "Source Tag".into(),
            to: "Organized Tag".into(),
            evidence_refs: vec!["sourceTags".into()],
            basis: "Explicit organization".into(),
            confidence: 0.9,
        },
        crate::ai::analysis::TagAliasSuggestion {
            from: "Not An Existing Tag".into(),
            to: "Invented".into(),
            evidence_refs: vec!["sourceTags".into()],
            basis: "Ineligible".into(),
            confidence: 0.8,
        },
    ];
    let ownership = details(&c)["ownedAvatarEvidence"].clone();
    c.publish_ai_tag_organization(
        id,
        &organized.catalogue_fingerprint,
        &aliases,
        "synthetic-model",
    )
    .unwrap();
    assert_eq!(details(&c)["category"], "Clothes");
    assert_eq!(details(&c)["sourceTags"], json!(["Source Tag"]));
    assert!(strings(&details(&c)["tags"]).contains(&"Organized Tag".into()));
    assert!(!strings(&details(&c)["tags"]).contains(&"Source Tag".into()));
    assert!(!strings(&details(&c)["tags"]).contains(&"Invented".into()));
    assert_eq!(details(&c)["ownedAvatarEvidence"], ownership);
    c.command(
        "overlay",
        json!({"ids":[id],"patch":{"tags":["Human Tag"]}}),
    )
    .unwrap();
    let organized = c.collect_ai_metadata(id).unwrap();
    c.publish_ai_tag_organization(
        id,
        &organized.catalogue_fingerprint,
        &aliases,
        "synthetic-model",
    )
    .unwrap();
    assert_eq!(details(&c)["tags"], json!(["Human Tag"]));
    assert!(
        strings(&c.get("overlay", id).unwrap().unwrap()["rejectedTags"])
            .contains(&"Organized Tag".into())
    );
    source["sourceTags"] = json!(["Next Source"]);
    c.put("product", id, source).unwrap();
    assert_eq!(
        c.publish_ai_tag_organization(
            id,
            &organized.catalogue_fingerprint,
            &aliases,
            "synthetic-model"
        )
        .unwrap_err()
        .to_string(),
        "AI_INPUT_STALE"
    );
    let prefs = c
        .command("preferences", json!({"patch":{"aiEnabled":true}}))
        .unwrap();
    assert_eq!(prefs["aiEnabled"], true);
    assert_eq!(prefs["aiScopes"], default_ai_scopes());
    let mut scopes = default_ai_scopes();
    scopes["images"] = json!(true);
    c.command("preferences", json!({"patch":{"aiScopes":scopes.clone()}}))
        .unwrap();
    assert_eq!(
        c.get("preferences", "app").unwrap().unwrap()["aiScopes"]["images"],
        true
    );
    scopes["unrecognizedScope"] = json!(true);
    assert!(c
        .command("preferences", json!({"patch":{"aiScopes":scopes}}))
        .is_err());
    assert!(c
        .command("preferences", json!({"patch":{"aiEnabled":"true"}}))
        .is_err());
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}
