
use super::*;

#[tokio::test]
async fn chat_entry_choice_does_not_change_active_or_classification_snapshot() {
    let dir = std::env::temp_dir().join(format!("nna-ai-entry-authored-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let service = AiService::new(&dir).unwrap();
    let first = AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        provider: "local".into(),
        endpoint: "http://localhost:1234/v1".into(),
        model: "authored-one".into(),
    }
    .validated()
    .unwrap();
    let second = AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        model: "authored-two".into(),
        ..first.clone()
    }
    .validated()
    .unwrap();
    provider::save_entry(&service.settings_path, first.clone()).unwrap();
    provider::save_entry(&service.settings_path, second.clone()).unwrap();
    provider::select_entry(&service.settings_path, first.entry_id.as_deref().unwrap()).unwrap();
    let chosen = service
        .config_for_entry(second.entry_id.as_deref())
        .await
        .unwrap();
    assert_eq!(chosen.model, "authored-two");
    assert_eq!(
        service.current_config().await.unwrap().model,
        "authored-one"
    );
    assert_eq!(
        service
            .chat_request_snapshot(Some(&chosen))
            .await
            .unwrap()
            .0
            .model,
        "authored-two"
    );
    assert_eq!(
        service
            .request_snapshot(Some(&chosen))
            .await
            .err()
            .unwrap()
            .to_string(),
        "ai_config_changed"
    );
    provider::remove_entry(&service.settings_path, second.entry_id.as_deref().unwrap()).unwrap();
    assert!(service.chat_request_snapshot(Some(&chosen)).await.is_err());
    drop(service);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_credentials_isolate_profiles_providers_and_endpoints() {
    let profile = "NekoNekoAsset/AI/synthetic-profile";
    let config = AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        ..AiConfig::default()
    };
    let duplicate = AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        ..config.clone()
    };
    assert!(config != duplicate);
    let official = config_credential_target(profile, &config);
    assert_ne!(config_credential_target(profile, &duplicate), official);
    assert_ne!(official, profile);
    assert_ne!(
        official,
        config_credential_target("another-profile", &config)
    );
    let other = AiConfig {
        endpoint: "https://another.invalid/v1".into(),
        ..config.clone()
    };
    assert_ne!(config_credential_target(profile, &other), official);
    let renamed_model = AiConfig {
        model: "gpt-4.1-mini".into(),
        ..config.clone()
    };
    assert_eq!(config_credential_target(profile, &renamed_model), official);
    let custom = AiConfig {
        provider: "custom".into(),
        ..config
    };
    assert_ne!(config_credential_target(profile, &custom), official);
}

#[test]
fn reply_payload_keeps_images_on_last_user_and_rejects_unbounded_or_assistant_tail() {
    let messages = [
        AiMessage {
            role: AiRole::User,
            content: "first".into(),
        },
        AiMessage {
            role: AiRole::Assistant,
            content: "earlier reply".into(),
        },
        AiMessage {
            role: AiRole::User,
            content: "selected evidence".into(),
        },
    ];
    let mut payload = reply_payload("grounded policy", &messages, None).unwrap();
    attach_images(
        &mut payload,
        &[AiImage {
            mime_type: "image/png".into(),
            data_base64: "c3ludGhldGlj".into(),
        }],
        &["i-synthetic".into()],
    )
    .unwrap();
    assert_eq!(payload["messages"][1]["content"], "first");
    assert_eq!(payload["messages"][2]["content"], "earlier reply");
    assert_eq!(
        payload["messages"][3]["content"].as_array().unwrap().len(),
        3
    );
    assert!(reply_payload("policy", &messages[..2], None).is_err());
    assert!(reply_payload(&"x".repeat(64 * 1024 + 1), &messages, None).is_err());
    assert_eq!(payload["store"], false);
    assert_eq!(payload["stream"], false);
}

#[tokio::test]
#[ignore = "Explicit opt-in synthetic OpenAI call using a root-owned isolated profile"]
async fn synthetic_organization_pipeline_diagnostic() -> Result<()> {
    ensure!(
        std::env::var("NNA_AI_SYNTHETIC_PROBE").as_deref() == Ok("1"),
        "synthetic_probe_opt_in_required"
    );
    let profile = std::env::var_os("NNA_AI_SYNTHETIC_PROFILE")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow!("synthetic_profile_required"))?;
    let local = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("synthetic_profile_invalid"))?
        .join(".local")
        .canonicalize()
        .map_err(|_| anyhow!("synthetic_profile_invalid"))?;
    let profile = profile
        .canonicalize()
        .map_err(|_| anyhow!("synthetic_profile_invalid"))?;
    ensure!(
        profile.is_dir() && profile != local && profile.starts_with(&local),
        "synthetic_profile_invalid"
    );
    let input = normalize_input(AnalysisInput {
        title: String::new(),
        source_tags: [
            "Unity",
            "VRChat",
            "animation",
            "cartoon",
            "shader",
            "shader synthetic",
            "toon shader",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        description: String::new(),
        rejected_tags: vec![],
        files: vec![],
    })?;
    let service = AiService::new(&profile)?;
    // Never read catalogue records. The secret stays inside the existing
    // vault/HTTP path; all diagnostics below are fixed aggregate facts.
    let response = post_json(
        service.key_snapshot().await?,
        &analysis_payload(&input, true),
        60,
        128 * 1024,
    )
    .await;
    let value = match response {
        Ok(value) => {
            println!("stage=transport result=ok");
            value
        }
        Err(error) => {
            let code = match error.to_string().as_str() {
                "ai_auth_failed" => "ai_auth_failed",
                "ai_permission_denied" => "ai_permission_denied",
                "ai_rate_limited" => "ai_rate_limited",
                "ai_quota_exceeded" => "ai_quota_exceeded",
                "ai_spend_limit_exceeded" => "ai_spend_limit_exceeded",
                "ai_timeout" => "ai_timeout",
                "ai_network" => "ai_network",
                "ai_malformed" => "ai_malformed",
                _ => "ai_unavailable",
            };
            println!("stage=transport result={code}");
            return Err(anyhow!(code));
        }
    };
    let choice = value.pointer("/choices/0");
    let finish = match choice.and_then(|choice| choice["finish_reason"].as_str()) {
        Some("stop") => "stop",
        Some("length") => "length",
        Some("content_filter") => "content_filter",
        _ => "other",
    };
    let refusal = choice
        .and_then(|choice| choice.pointer("/message/refusal"))
        .and_then(Value::as_str);
    let content = choice
        .and_then(|choice| choice.pointer("/message/content"))
        .and_then(Value::as_str);
    println!(
        "stage=envelope finish={finish} contentPresent={} contentBytes={} refusalPresent={}",
        content.is_some(),
        content.map_or(0, str::len),
        refusal.is_some_and(|text| !text.trim().is_empty())
    );
    let body = content.and_then(|content| serde_json::from_str::<Value>(content).ok());
    let count = |field: &str| {
        body.as_ref()
            .and_then(|body| body[field].as_array())
            .map_or(0, Vec::len)
    };
    let aliases: Vec<_> = body
        .as_ref()
        .and_then(|body| body["tagAliases"].as_array())
        .into_iter()
        .flatten()
        .collect();
    let labels: BTreeSet<_> = input
        .source_tags
        .iter()
        .map(|tag| crate::taxonomy::normalize(tag))
        .collect();
    let unknown_from = aliases
        .iter()
        .filter(|alias| {
            !alias["from"]
                .as_str()
                .is_some_and(|from| labels.contains(&crate::taxonomy::normalize(from)))
        })
        .count();
    let unknown_proof = aliases
        .iter()
        .flat_map(|alias| alias["evidenceRefs"].as_array().into_iter().flatten())
        .filter(|reference| reference.as_str() != Some("sourceTags"))
        .count();
    let empty_basis = aliases
        .iter()
        .filter(|alias| {
            !alias["basis"]
                .as_str()
                .is_some_and(|basis| !basis.trim().is_empty())
        })
        .count();
    println!(
            "stage=shape jsonAccepted={} categoryNull={} tags={} aliases={} urls={} unknownFrom={} unknownProof={} emptyBasis={}",
            body.is_some(),
            body.as_ref().is_some_and(|body| body["category"].is_null()),
            count("tags"),
            count("tagAliases"),
            count("sourceUrlCandidates"),
            unknown_from,
            unknown_proof,
            empty_basis
        );
    let parsed = content.is_some_and(|content| parse_output(content, refusal, &input).is_ok());
    let accepted = analysis_response(&value, &input, true).is_ok();
    println!("stage=validation parseOutputAccepted={parsed} analysisResponseAccepted={accepted}");
    ensure!(accepted, "ai_malformed");
    Ok(())
}

#[test]
fn typed_requests_and_status_exclude_secret_and_scope_inputs() {
    for action in ["key_status", "test_connection", "providers", "list_entries"] {
        assert!(serde_json::from_value::<AiRequest>(json!({"action":action})).is_ok());
        assert!(serde_json::from_value::<AiRequest>(
            json!({"action":action,"key":"synthetic-key"})
        )
        .is_err());
    }
    assert!(serde_json::from_value::<AiRequest>(json!({"action":"test_draft","provider":"openai","endpoint":"https://example.invalid/v1","model":"synthetic","key":"synthetic-key"})).is_ok());
    for value in [
        json!({"action":"set_key","key":"synthetic-key"}),
        json!({"action":"configure","provider":"openai","endpoint":"https://example.invalid","model":"synthetic"}),
        json!({"action":"test_connection","productIds":["private-product"]}),
        json!({"action":"test_entry","entryId":"synthetic","key":"synthetic-key"}),
    ] {
        assert!(serde_json::from_value::<AiRequest>(value).is_err());
    }
    assert_eq!(
        serde_json::to_value(key_status(true)).unwrap(),
        json!({"entryId":null,"configured":false,"provider":"openai","endpoint":"https://api.openai.com/v1","model":"gpt-4o-mini","keyPresent":true,"visionSupported":true,"keyRequired":true})
    );
}

#[test]
fn draft_test_proof_requires_exact_config_key_and_is_single_use() {
    let (config, key, digest) = draft_config(
        "openai".into(),
        "https://example.invalid/v1".into(),
        "synthetic".into(),
        "synthetic-key".into(),
    )
    .unwrap();
    let now = Instant::now();
    let mut proofs = BTreeMap::new();
    proofs.insert(
        "good".into(),
        DraftProof {
            digest,
            tested_at: now,
        },
    );
    consume_draft_proof(&mut proofs, "good", digest, now).unwrap();
    assert!(consume_draft_proof(&mut proofs, "good", digest, now).is_err());
    for changed in [
        AiConfig {
            model: "changed".into(),
            ..config.clone()
        },
        AiConfig {
            endpoint: "https://other.invalid/v1".into(),
            ..config.clone()
        },
        AiConfig {
            provider: "custom".into(),
            ..config.clone()
        },
    ] {
        proofs.insert(
            "changed".into(),
            DraftProof {
                digest,
                tested_at: now,
            },
        );
        assert!(consume_draft_proof(
            &mut proofs,
            "changed",
            draft_digest(&changed, key.as_deref()),
            now
        )
        .is_err());
    }
    proofs.insert(
        "key".into(),
        DraftProof {
            digest,
            tested_at: now,
        },
    );
    assert!(consume_draft_proof(
        &mut proofs,
        "key",
        draft_digest(&config, Some("different-key")),
        now
    )
    .is_err());
    let (_, _, changed_spacing) = draft_config(
        "openai".into(),
        " https://example.invalid/v1 ".into(),
        "synthetic".into(),
        "synthetic-key".into(),
    )
    .unwrap();
    assert_ne!(digest, changed_spacing);
    proofs.insert(
        "expired".into(),
        DraftProof {
            digest,
            tested_at: now,
        },
    );
    assert!(consume_draft_proof(
        &mut proofs,
        "expired",
        digest,
        now + DRAFT_TEST_TTL + Duration::from_secs(1)
    )
    .is_err());
}

#[test]
fn fixed_connection_payload_is_synthetic_bounded_and_not_stored() {
    assert_eq!(
        connection_payload(),
        json!({
            "model":"gpt-4o-mini",
            "messages":[{"role":"user","content":"Reply with exactly OK and nothing else."}],
            "max_completion_tokens":8,"store":false,"stream":false
        })
    );
    assert_eq!(
        valid_key("  synthetic-key  ".into()).unwrap(),
        "synthetic-key"
    );
    for key in [
        String::new(),
        "key\r\nAuthorization: other".into(),
        "绉樺瘑".into(),
        "x".repeat(MAX_KEY_BYTES + 1),
    ] {
        assert_eq!(valid_key(key).unwrap_err().to_string(), "ai_key_invalid");
    }
}

#[test]
fn billing_limits_are_distinct_from_quota_rate_and_auth_failures() {
    assert_eq!(
        provider_response_error("deepseek", 402, &Value::Null),
        "ai_quota_exceeded"
    );
    for (source, expected) in [
        ("openrouter_key_limit", "ai_spend_limit_exceeded"),
        ("openrouter_credits", "ai_quota_exceeded"),
        ("openrouter_in_flight_budget", "ai_rate_limited"),
    ] {
        assert_eq!(
            provider_response_error(
                "openrouter",
                402,
                &json!({"error":{"metadata":{"limit_source":source},"message":"Never expose this private response"}})
            ),
            expected
        );
    }
    assert_eq!(
        provider_response_error("openrouter", 402, &Value::Null),
        "ai_quota_exceeded"
    );
    assert_eq!(
        provider_response_error("openai", 402, &Value::Null),
        "ai_malformed"
    );
    for code in [
        "project_spend_limit_exceeded",
        "organization_spend_limit_exceeded",
    ] {
        assert_eq!(
            response_error(
                429,
                &json!({"error":{"code":code,"type":"insufficient_quota"}})
            ),
            "ai_spend_limit_exceeded"
        );
    }
    for code in [
        "insufficient_quota",
        "credit_balance_exhausted",
        "usage_limit_exceeded",
        "organization_usage_limit_exceeded",
    ] {
        assert_eq!(
            response_error(429, &json!({"error":{"code":code}})),
            "ai_quota_exceeded"
        );
    }
    assert_eq!(
        response_error(
            429,
            &json!({"error":{"code":"rate_limit_exceeded","type":"insufficient_quota"}})
        ),
        "ai_rate_limited"
    );
    assert_eq!(
        response_error(401, &json!({"error":{"message":"never expose this"}})),
        "ai_auth_failed"
    );
    assert_eq!(response_error(403, &Value::Null), "ai_permission_denied");
    assert_eq!(response_error(503, &Value::Null), "ai_unavailable");
    assert_eq!(response_error(302, &Value::Null), "ai_malformed");
    assert_eq!(
        validate_success(&json!({"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}))
            .unwrap(),
        ()
    );
    assert_eq!(
        validate_success(&json!({"choices":[{"message":{"refusal":"blocked"}}]}))
            .unwrap_err()
            .to_string(),
        "ai_refused"
    );
    assert_eq!(
        validate_success(&json!({"unrelated":"response"}))
            .unwrap_err()
            .to_string(),
        "ai_malformed"
    );
}

#[test]
fn credential_targets_are_profile_specific_without_disclosing_paths() {
    let first = credential_target(Path::new("F:/Synthetic/One"));
    let second = credential_target(Path::new("F:/Synthetic/Two"));
    assert_ne!(first, second);
    assert_eq!(first.len(), "NekoNekoAsset/AI/".len() + 64);
    assert!(!first.contains("Synthetic"));
    #[cfg(windows)]
    assert_eq!(first, credential_target(Path::new("f:\\synthetic\\one\\")));
}

#[test]
fn analysis_transport_uses_fixed_schema_and_rejects_unobserved_urls() {
    let input = AnalysisInput {
        title: "Synthetic jacket".into(),
        source_tags: vec![],
        description: String::new(),
        rejected_tags: vec![],
        files: vec![analysis::FileEvidence {
            evidence_ref: "e-synthetic".into(),
            file_name: "README.md".into(),
            text_excerpt: "Synthetic jacket instructions".into(),
            candidate_public_urls: vec!["https://maker.gumroad.com/l/jacket".into()],
        }],
    };
    let payload = analysis_payload(&input, false);
    assert_eq!(payload["model"], "gpt-4o-mini");
    assert_eq!(payload["max_completion_tokens"], 4096);
    assert_eq!(payload["store"], false);
    assert_eq!(payload["stream"], false);
    assert_eq!(payload["response_format"]["json_schema"]["strict"], true);
    assert_eq!(payload["messages"][0]["role"], "system");
    assert_eq!(
        serde_json::from_str::<Value>(payload["messages"][1]["content"].as_str().unwrap()).unwrap(),
        serde_json::to_value(&input).unwrap()
    );
    let mut output = json!({"category":null,"tags":[],"tagAliases":[],"sourceUrlCandidates":[{
        "url":"https://maker.gumroad.com/l/jacket","evidenceRefs":["e-synthetic"],
        "basis":"Literal reference in the selected README","confidence":0.8
    }]});
    let response = |output: &Value| json!({"choices":[{"message":{"content":output.to_string()},"finish_reason":"stop"}]});
    assert!(analysis_response(&response(&output), &input, false).is_ok());
    output["sourceUrlCandidates"][0]["evidenceRefs"] = json!(["title"]);
    assert!(analysis_response(&response(&output), &input, false).is_err());
    output["sourceUrlCandidates"][0]["evidenceRefs"] = json!(["e-synthetic"]);
    output["sourceUrlCandidates"][0]["url"] = json!("https://maker.gumroad.com/l/invented");
    assert_eq!(
        analysis_response(&response(&output), &input, false)
            .err()
            .unwrap()
            .to_string(),
        "ai_malformed"
    );
    assert_eq!(
        analysis_response(
            &json!({"choices":[{"message":{"refusal":"Refused"},"finish_reason":"stop"}]}),
            &input,
            false
        )
        .err()
        .unwrap()
        .to_string(),
        "ai_refused"
    );
    let mut truncated = response(&output);
    truncated["choices"][0]["finish_reason"] = json!("length");
    assert_eq!(
        analysis_response(&truncated, &input, false)
            .err()
            .unwrap()
            .to_string(),
        "ai_malformed"
    );
    let organization = AnalysisInput {
        title: String::new(),
        source_tags: vec!["shadder".into()],
        description: String::new(),
        rejected_tags: vec![],
        files: vec![],
    };
    assert!(
        analysis_payload(&organization, true)["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("tag-only organization")
    );
    let organization_payload = analysis_payload(&organization, true);
    let organization_prompt = organization_payload["messages"][0]["content"]
        .as_str()
        .unwrap();
    assert!(organization_prompt.contains("evidenceRefs:[\"sourceTags\"]"));
    assert!(!organization_prompt.contains("title"));
    let organization_schema = &organization_payload["response_format"]["json_schema"];
    assert_eq!(organization_schema["name"], "tag_organization");
    assert_eq!(organization_schema["strict"], true);
    assert_eq!(
        organization_schema["schema"]["properties"]["category"],
        json!({"type":"null"})
    );
    assert_eq!(
        organization_schema["schema"]["properties"]["tags"]["maxItems"],
        0
    );
    assert_eq!(
        organization_schema["schema"]["properties"]["sourceUrlCandidates"]["maxItems"],
        0
    );
    assert_eq!(
        organization_schema["schema"]["properties"]["tagAliases"],
        structured_output_schema()["schema"]["properties"]["tagAliases"]
    );
    assert_eq!(
        payload["response_format"]["json_schema"],
        structured_output_schema()
    );
    assert!(
        !analysis_payload(&organization, false)["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("tag-only organization")
    );
    let mut aliases = json!({"category":null,"tags":[],"tagAliases":[{"from":"shadder","to":"Shader","evidenceRefs":["sourceTags"],"basis":"Spelling correction","confidence":0.95}],"sourceUrlCandidates":[]});
    assert!(analysis_response(&response(&aliases), &organization, true).is_ok());
    aliases["tagAliases"][0]["evidenceRefs"] = json!(["title"]);
    assert!(analysis_response(&response(&aliases), &organization, true).is_err());
    aliases["tagAliases"][0]["evidenceRefs"] = json!(["sourceTags"]);
    aliases["tagAliases"][0]["from"] = json!("invented label");
    assert!(analysis_response(&response(&aliases), &organization, true).is_err());
    aliases["tagAliases"] = json!([]);
    aliases["category"] = json!({"category":"Shader","evidenceRefs":["sourceTags"],"basis":"Shader source tag supports classification","confidence":0.95});
    assert!(analysis_response(&response(&aliases), &organization, true).is_err());
    aliases["tags"] = json!([{"label":"Shader","evidenceRefs":["sourceTags"],"basis":"Shader source tag supports classification","confidence":0.95}]);
    let classification = analysis_response(&response(&aliases), &organization, false).unwrap();
    assert_eq!(classification.category.unwrap().category, "Shader");
    assert_eq!(classification.tags[0].label, "Shader");
    assert!(analysis_response(&response(&aliases), &organization, true).is_err());
}
