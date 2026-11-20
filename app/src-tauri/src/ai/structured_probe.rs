use super::*;

fn safe_code(error: &anyhow::Error) -> &'static str {
    match error.to_string().as_str() {
        "ai_key_missing" => "ai_key_missing",
        "ai_key_invalid" => "ai_key_invalid",
        "ai_secure_storage_unavailable" => "ai_secure_storage_unavailable",
        "ai_auth_failed" => "ai_auth_failed",
        "ai_permission_denied" => "ai_permission_denied",
        "ai_rate_limited" => "ai_rate_limited",
        "ai_quota_exceeded" => "ai_quota_exceeded",
        "ai_spend_limit_exceeded" => "ai_spend_limit_exceeded",
        "ai_timeout" => "ai_timeout",
        "ai_network" => "ai_network",
        "ai_refused" => "ai_refused",
        "ai_malformed" => "ai_malformed",
        _ => "ai_unavailable",
    }
}

#[tokio::test]
#[ignore = "Lead-only opt-in synthetic structured requests using explicitly selected protected credentials"]
async fn saved_provider_structured_output_diagnostic() -> Result<()> {
    ensure!(
        std::env::var("NNA_AI_STRUCTURED_PROBE").as_deref() == Ok("1"),
        "ai_probe_opt_in_required"
    );
    let mode = std::env::var("NNA_AI_STRUCTURED_MODE").unwrap_or_default();
    ensure!(
        matches!(mode.as_str(), "inventory" | "run"),
        "ai_probe_mode_required"
    );
    let profile = std::env::var_os("NNA_AI_STRUCTURED_PROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("ai_probe_profile_required"))?;
    ensure!(
        profile.is_absolute() && profile.is_dir(),
        "ai_probe_profile_invalid"
    );
    let service = AiService::new(&profile).map_err(|_| anyhow!("ai_probe_profile_unavailable"))?;
    let (configs, _) = provider::list_entries(&service.settings_path)
        .map_err(|_| anyhow!("ai_probe_settings_unavailable"))?;
    let configured_entries = configs.len();
    let mut groups: BTreeMap<(String, String, String), Vec<AiConfig>> = BTreeMap::new();
    for config in configs {
        groups
            .entry((
                config.provider.clone(),
                config.endpoint.clone(),
                config.model.clone(),
            ))
            .or_default()
            .push(config);
    }
    let distinct_targets = groups.len();
    // Only fixed provider labels and aggregate counts leave this process.
    // No entry IDs, models, endpoints, keys, raw responses or catalogue reads.
    let mut reports = BTreeMap::new();
    for preset in provider::presets() {
        reports.insert(preset.id, json!({"provider":preset.id,"configuredEntries":0,"distinctTargets":0,"keylessLocalTargets":0,"credentialReadyTargets":0,"requests":0,"passed":0,"unavailable":{},"failures":{}}));
    }
    let input = normalize_input(AnalysisInput {
        title: String::new(),
        source_tags: vec![],
        description: String::new(),
        rejected_tags: vec![],
        files: vec![],
    })?;
    let payload = analysis_payload(&input, false);
    let mut requests = 0usize;
    let mut passed = 0usize;
    for ((provider, _, _), configs) in groups {
        let report = reports
            .get_mut(provider.as_str())
            .ok_or_else(|| anyhow!("ai_probe_settings_unavailable"))?;
        report["configuredEntries"] =
            json!(report["configuredEntries"].as_u64().unwrap() + configs.len() as u64);
        report["distinctTargets"] = json!(report["distinctTargets"].as_u64().unwrap() + 1);
        if !configs[0].key_required() {
            report["keylessLocalTargets"] =
                json!(report["keylessLocalTargets"].as_u64().unwrap() + 1);
        }
        let mut selected = None;
        let mut unavailable = "ai_key_missing";
        for config in configs {
            match service.config_key(&config).await {
                Ok(key) => {
                    selected = Some((config, key));
                    break;
                }
                Err(error) => unavailable = safe_code(&error),
            }
        }
        let Some((config, key)) = selected else {
            let count = report["unavailable"][unavailable].as_u64().unwrap_or(0);
            report["unavailable"][unavailable] = json!(count + 1);
            continue;
        };
        report["credentialReadyTargets"] =
            json!(report["credentialReadyTargets"].as_u64().unwrap() + 1);
        if mode == "inventory" {
            continue;
        }
        requests += 1;
        report["requests"] = json!(report["requests"].as_u64().unwrap() + 1);
        // Exactly one request per saved provider/endpoint/model. The normal
        // cookie-free, no-redirect/no-retry production transport owns the key.
        let result = post_config_json(&config, key, &payload, 60, 128 * 1024)
            .await
            .and_then(|value| analysis_response(&value, &input, false))
            .and_then(|output| {
                ensure!(
                    output.category.is_none()
                        && output.tags.is_empty()
                        && output.tag_aliases.is_empty()
                        && output.source_url_candidates.is_empty(),
                    "ai_malformed"
                );
                Ok(())
            });
        match result {
            Ok(()) => {
                passed += 1;
                report["passed"] = json!(report["passed"].as_u64().unwrap() + 1);
            }
            Err(error) => {
                let code = safe_code(&error);
                let count = report["failures"][code].as_u64().unwrap_or(0);
                report["failures"][code] = json!(count + 1);
            }
        }
    }
    println!(
        "AI_STRUCTURED_PROBE {}",
        json!({"mode":mode,"syntheticOnly":true,"catalogueRead":false,"imagesSent":false,"automaticRetries":false,"configuredEntries":configured_entries,"distinctTargets":distinct_targets,"requests":requests,"passed":passed,"completeForConfiguredTargets":mode == "run" && requests > 0 && requests == distinct_targets && passed == requests,"providers":reports.into_values().collect::<Vec<_>>()})
    );
    if mode == "run" {
        ensure!(requests > 0, "ai_probe_no_available_entries");
        ensure!(
            requests == distinct_targets,
            "ai_probe_credentials_unavailable"
        );
        ensure!(passed == requests, "ai_probe_structured_failure");
    }
    Ok(())
}
