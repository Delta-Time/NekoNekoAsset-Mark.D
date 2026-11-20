
use super::*;
#[test]
fn overlapping_jobs_deduplicate_only_matching_mode_and_classification_scopes() {
    let scopes = AiScopes::default();
    let config = AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        ..AiConfig::default()
    };
    let active = json!({"id":"active","type":"ai:classify","state":"running","aiMode":"classify","aiConfig":config,"aiScopes":scopes,"productIds":["p1","p2"],"completedProductIds":["p1"]});
    assert_eq!(
        enqueue_selection(
            &["p1".into()],
            &[active.clone()],
            AiMode::Classify,
            &scopes,
            &config
        ),
        (vec![], Some("active".into()))
    );
    assert_eq!(
        enqueue_selection(
            &["p2".into(), "p3".into()],
            &[active.clone()],
            AiMode::Classify,
            &scopes,
            &config
        ),
        (vec!["p3".into()], None)
    );
    let different = AiScopes {
        files: false,
        ..Default::default()
    };
    assert_eq!(
        enqueue_selection(
            &["p1".into()],
            &[active.clone()],
            AiMode::Classify,
            &different,
            &config
        ),
        (vec!["p1".into()], None)
    );
    // Identical provider/model endpoints still have independent entry identities.
    for different_config in [
        AiConfig {
            entry_id: Some(uuid::Uuid::new_v4().to_string()),
            ..config.clone()
        },
        AiConfig {
            model: "different-model".into(),
            ..config.clone()
        },
    ] {
        assert_eq!(
            enqueue_selection(
                &["p1".into()],
                &[active.clone()],
                AiMode::Classify,
                &scopes,
                &different_config
            ),
            (vec!["p1".into()], None)
        );
    }
    let mut cancelled = active.clone();
    cancelled["state"] = json!("cancelled");
    assert_eq!(
        enqueue_selection(
            &["p1".into()],
            &[cancelled],
            AiMode::Classify,
            &scopes,
            &config
        ),
        (vec!["p1".into()], None)
    );
    let mut organization = active.clone();
    organization["type"] = json!("ai:organize");
    organization["aiMode"] = json!("organize");
    assert_eq!(
        enqueue_selection(
            &["p1".into()],
            &[organization],
            AiMode::Organize,
            &different,
            &config
        ),
        (vec![], Some("active".into()))
    );
    let other = json!({"id":"other","type":"ai:classify","state":"queued","aiMode":"classify","aiConfig":config,"aiScopes":scopes,"productIds":["p3"]});
    let (remaining, covering) = enqueue_selection(
        &["p1".into(), "p3".into()],
        &[active, other],
        AiMode::Classify,
        &scopes,
        &config,
    );
    assert!(remaining.is_empty());
    assert!(covering.is_some());
}

#[test]
fn scopes_allow_image_only_and_remove_unselected_data() {
    let input = AnalysisInput {
        title: "synthetic".into(),
        source_tags: vec!["Texture".into()],
        description: "safe".into(),
        rejected_tags: vec!["Rejected".into()],
        files: vec![],
    };
    let scopes = AiScopes {
        title: false,
        tags: false,
        description: true,
        files: false,
        images: false,
    };
    let selected = selected_input(input.clone(), &scopes).unwrap();
    assert!(
        selected.title.is_empty()
            && selected.source_tags.is_empty()
            && selected.rejected_tags.is_empty()
    );
    assert_eq!(selected.description, "safe");
    let mut image_only = scopes.clone();
    image_only.description = false;
    image_only.images = true;
    let selected = selected_input(input.clone(), &image_only).unwrap();
    assert!(
        selected.title.is_empty() && selected.description.is_empty() && selected.files.is_empty()
    );
    image_only.images = false;
    assert!(selected_input(input, &image_only).is_err());
}
#[test]
fn preview_evidence_keeps_image_inside_existing_input_limits() {
    let mut input = AnalysisInput {
        title: String::new(),
        description: String::new(),
        source_tags: vec![],
        rejected_tags: vec![],
        files: (0..64)
            .map(|n| FileEvidence {
                evidence_ref: format!("f-{n}"),
                file_name: format!("file-{n}.txt"),
                text_excerpt: "x".repeat(2048),
                candidate_public_urls: vec![],
            })
            .collect(),
    };
    let mut bytes = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let image = AiImage::from_bytes("image/png", &bytes).unwrap();
    add_preview_evidence(&mut input, &image).unwrap();
    assert_eq!(input.files[0].evidence_ref, image.evidence_ref());
    assert!(input.files.len() <= 64 && serde_json::to_vec(&input).unwrap().len() <= 128 * 1024);
    let path =
        std::env::temp_dir().join(format!("nna-authored-preview-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).unwrap();
    assert!(read_preview(&path).is_ok());
    std::fs::write(&path, b"not an image").unwrap();
    assert!(read_preview(&path).is_err());
    std::fs::remove_file(path).unwrap();
}
#[test]
fn job_contract_rejects_keys_and_arbitrary_provider_settings() {
    assert!(serde_json::from_value::<AiJobRequest>(
        json!({"action":"cancel","jobId":"synthetic","key":"secret"})
    )
    .is_err());
    assert!(serde_json::from_value::<AiScopes>(json!({"title":true,"tags":true,"description":true,"files":true,"images":false,"endpoint":"https://invalid.example"})).is_err());
}
