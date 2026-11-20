use super::*;
#[test]
fn gemini_structured_output_uses_generate_content_mime_enum() {
    let config = AiConfig {
        entry_id: None,
        provider: "gemini".into(),
        endpoint: "https://example.invalid/v1beta".into(),
        model: "synthetic-model".into(),
    };
    let schema = crate::ai::analysis::structured_output_schema();
    let payload = request_payload(&config, json!({"messages":[{"role":"user","content":"synthetic"}],"max_completion_tokens":4096,"response_format":{"type":"json_schema","json_schema":schema.clone()}})).unwrap();
    assert_eq!(
        payload["generationConfig"]["responseFormat"],
        json!({"text":{"mimeType":"APPLICATION_JSON","schema":schema["schema"]}})
    );
    assert!(payload["generationConfig"]
        .get("responseMimeType")
        .is_none());
    assert!(payload["generationConfig"]
        .get("responseJsonSchema")
        .is_none());
    assert_eq!(payload["generationConfig"]["maxOutputTokens"], 4096);
}
#[test]
fn anthropic_wire_bounds_transform_does_not_relax_native_confidence_validation() {
    let schema = crate::ai::analysis::structured_output_schema();
    let config = AiConfig {
        entry_id: None,
        provider: "anthropic".into(),
        endpoint: "https://example.invalid/v1".into(),
        model: "synthetic-model".into(),
    };
    let payload=request_payload(&config,json!({"messages":[{"role":"user","content":"synthetic"}],"max_completion_tokens":4096,"response_format":{"type":"json_schema","json_schema":schema.clone()}})).unwrap();
    let wire = &payload["output_config"]["format"]["schema"];
    assert!(wire["properties"]["tags"].get("maxItems").is_none());
    assert!(
        wire["properties"]["tags"]["items"]["properties"]["confidence"]
            .get("maximum")
            .is_none()
    );
    assert!(
        wire["properties"]["tags"]["items"]["properties"]["confidence"]["description"]
            .as_str()
            .unwrap()
            .contains("maximum: 1")
    );
    assert_eq!(
        schema["schema"]["properties"]["tags"]["items"]["properties"]["confidence"]["maximum"],
        1
    );
    let input = crate::ai::analysis::AnalysisInput {
        title: "Synthetic jacket".into(),
        source_tags: vec![],
        description: String::new(),
        rejected_tags: vec![],
        files: vec![],
    };
    let mut output = json!({"category":null,"tags":[{"label":"jacket","evidenceRefs":["title"],"basis":"Synthetic title","confidence":1.2}],"tagAliases":[],"sourceUrlCandidates":[]});
    assert!(crate::ai::analysis::parse_output(&output.to_string(), None, &input).is_err());
    output["tags"][0]["confidence"] = json!(0.8);
    assert!(crate::ai::analysis::parse_output(&output.to_string(), None, &input).is_ok());
}
#[test]
fn provider_settings_preserve_models_without_credentials() {
    let directory = std::env::temp_dir().join(format!("nna-ai-config-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("ai-settings.json");
    assert_eq!(
        load_config(&path).err().unwrap().to_string(),
        "ai_not_configured"
    );
    assert!(list_entries(&path).unwrap().0.is_empty());
    let first_id = uuid::Uuid::new_v4().to_string();
    let second_id = uuid::Uuid::new_v4().to_string();
    let first = AiConfig {
        entry_id: Some(first_id.clone()),
        model: "gpt-4.1-mini".into(),
        ..AiConfig::default()
    };
    let second = AiConfig {
        entry_id: Some(second_id.clone()),
        ..first.clone()
    };
    save_entry(&path, first.clone()).unwrap();
    save_entry(&path, second.clone()).unwrap();
    assert!(load_config(&path).unwrap() == second);
    assert_eq!(list_entries(&path).unwrap().0.len(), 2);
    select_entry(&path, &first_id).unwrap();
    assert!(load_config(&path).unwrap() == first);
    assert!(select_entry(&path, "missing").is_err());
    assert!(load_config(&path).unwrap() == first);
    remove_entry(&path, &second_id).unwrap();
    assert!(load_config(&path).unwrap() == first);
    save_entry(&path, second).unwrap();
    remove_entry(&path, &second_id).unwrap();
    assert!(load_config(&path).unwrap() == first);
    let bytes = fs::read(&path).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("key"));
    remove_entry(&path, &first_id).unwrap();
    assert_eq!(list_entries(&path).unwrap().1, None);
    assert_eq!(
        load_config(&path).err().unwrap().to_string(),
        "ai_not_configured"
    );
    fs::write(
        &path,
        r#"{"version":1,"activeProvider":"openai","profiles":{}}"#,
    )
    .unwrap();
    assert!(list_entries(&path).is_err()); // Reject old formats; never migrate.
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
#[test]
fn configurations_reject_secret_urls_and_remote_insecure_local_servers() {
    for endpoint in [
        "https://key@example.invalid/v1",
        "https://example.invalid/v1?key=secret",
        "http://example.invalid/v1",
    ] {
        assert!(AiConfig {
            endpoint: endpoint.into(),
            ..AiConfig::default()
        }
        .validated()
        .is_err());
    }
    assert!(AiConfig {
        entry_id: None,
        provider: "local".into(),
        endpoint: "http://127.0.0.1:1234/v1".into(),
        model: "local-model".into()
    }
    .validated()
    .is_ok());
    assert!(AiConfig {
        entry_id: None,
        provider: "local".into(),
        endpoint: "https://example.invalid/v1".into(),
        model: "local-model".into()
    }
    .validated()
    .is_err());
}
#[test]
fn native_adapters_preserve_schema_and_image_content_without_url_queries() {
    let payload = json!({"model":"old","messages":[{"role":"system","content":"policy"},{"role":"user","content":[{"type":"text","text":"evidence"},{"type":"image_url","image_url":{"url":"data:image/png;base64,c3ludGhldGlj"}}]}],"max_completion_tokens":8,"response_format":{"type":"json_schema","json_schema":{"name":"test","strict":true,"schema":{"type":"object"}}}});
    for (provider, wire) in [("anthropic", AiWire::Anthropic), ("gemini", AiWire::Gemini)] {
        let config = AiConfig {
            entry_id: None,
            provider: provider.into(),
            endpoint: "https://example.invalid/v1".into(),
            model: "synthetic-model".into(),
        };
        let output = request_payload(&config, payload.clone()).unwrap();
        assert!(!output.to_string().contains("image_url"));
        assert!(output.to_string().contains("c3ludGhldGlj"));
        assert_eq!(config.wire(), wire);
        assert!(!config.request_url().contains('?'));
    }
    let anthropic = response_payload(
        AiWire::Anthropic,
        json!({"content":[{"type":"text","text":"{}"}],"stop_reason":"end_turn"}),
    )
    .unwrap();
    assert_eq!(anthropic["choices"][0]["finish_reason"], "stop");
    let gemini=response_payload(AiWire::Gemini,json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private reasoning"},{"text":"{}"}]},"finishReason":"STOP"}]})).unwrap();
    assert_eq!(gemini["choices"][0]["message"]["content"], "{}");
}
