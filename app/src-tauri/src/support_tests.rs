use super::*;

#[test]
fn capture_lifecycle_distinguishes_execution_from_report_version_without_inventing_legacy() {
    let mut job = json!({"id":"PRIVATE_JOB","type":"boothSync","state":"needsAction",
            "errorCode":"SYNC_INCOMPLETE","progress":{"stage":"needsAction","pages":7}});
    let legacy = parsed(&job, &[], &[]);
    assert_eq!(legacy["app"]["versionScope"], "reportGeneration");
    assert_eq!(legacy["capture"]["status"], "notRecorded");
    assert_eq!(legacy["capture"]["executionAppVersion"], "unknown");
    assert!(legacy.get("sourceCoverage").is_none());
    assert!(legacy.get("checkpoints").is_none());
    reset_attempt(&mut job);
    assert_eq!(parsed(&job, &[], &[])["capture"]["status"], "notStarted");
    begin_attempt(&mut job);
    let started = parsed(&job, &[], &[]);
    assert_eq!(started["capture"]["status"], "missingAfterStart");
    assert_eq!(
        started["capture"]["executionAppVersion"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(job["progress"]["pages"], 7);
    job["supportDiagnostics"] = json!({"version":1,"lastStage":"publishing"});
    end_attempt(&mut job);
    let captured = parsed(&job, &[], &[]);
    assert_eq!(captured["capture"]["status"], "available");
    assert_eq!(captured["capture"]["attemptState"], "finished");
    job["supportCapture"]["executionAppVersion"] = json!("PRIVATE_VERSION_0.2.3");
    assert_eq!(
        parsed(&job, &[], &[])["capture"]["executionAppVersion"],
        "unknown"
    );
    assert!(!report(&job, &[], &[]).contains("PRIVATE"));
    job["supportDiagnostics"]["version"] = json!(2);
    assert_eq!(
        parsed(&job, &[], &[])["capture"]["status"],
        "unsupportedCapture"
    );
}

#[test]
fn prior_public_capture_keeps_its_execution_version_after_upgrade() {
    let mut job = json!({"type":"boothSync","state":"completed",
            "supportCapture":{"version":1,"state":"finished","executionAppVersion":"0.2.4"},
            "supportDiagnostics":{"version":1,"lastStage":"publishing"}});
    let value = parsed(&job, &[], &[]);
    assert_eq!(value["app"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["app"]["versionScope"], "reportGeneration");
    assert_eq!(value["capture"]["executionAppVersion"], "0.2.4");
    assert_eq!(value["capture"]["attemptState"], "finished");
    assert_eq!(value["capture"]["status"], "available");
    job["supportCapture"]["executionAppVersion"] = json!("0.2.5");
    assert_eq!(
        parsed(&job, &[], &[])["capture"]["executionAppVersion"],
        "0.2.5"
    );
    job["supportCapture"]["executionAppVersion"] = json!("0.2.6");
    assert_eq!(
        parsed(&job, &[], &[])["capture"]["executionAppVersion"],
        "0.2.6"
    );
    job["supportCapture"]["executionAppVersion"] = json!("0.2.4-PRIVATE_SESSION_VERSION");
    assert_eq!(
        parsed(&job, &[], &[])["capture"]["executionAppVersion"],
        "unknown"
    );
    assert!(!report(&job, &[], &[]).contains("PRIVATE"));
}

#[test]
fn source_reason_closure_and_language_projection_never_echo_dynamic_values() {
    let job = json!({"type":"boothSync","supportDiagnostics":{"version":1,"sources":{"purchase":{
            "reasons":{"SOURCE_HEAD_CHANGED":1,"PRIVATE_REASON":2,"SOURCE_MANIFEST_INCOMPLETE":"PRIVATE_COUNT"},
            "reasonScope":"PRIVATE_SCOPE","parserCalls":3,"htmlOther":3,"htmlLanguage":"PRIVATE_LANGUAGE",
            "lastClosure":{"scope":"PRIVATE_SCOPE","pendingRequests":0,"unresolvedRows":2,
                "enrichmentPending":0,"incompleteProducts":1,"terminalSeen":true,"membershipUnbound":false,
                "headStable":false,"url":"PRIVATE_URL"}}}}});
    let text = report(&job, &[], &[]);
    assert!(!text.contains("PRIVATE"));
    let value: Value = serde_json::from_str(&text).unwrap();
    let source = &value["supportDiagnostics"]["sources"]["purchase"];
    assert_eq!(source["reasons"]["SOURCE_HEAD_CHANGED"], 1);
    assert_eq!(source["reasons"]["UNKNOWN_ERROR"], 2);
    assert!(source["reasons"]["SOURCE_MANIFEST_INCOMPLETE"].is_null());
    assert_eq!(
        source["lastClosure"]["scope"],
        "latestObservedSourceClosure"
    );
    assert_eq!(source["lastClosure"]["headStable"], false);
    assert_eq!(source["lastClosure"]["incompleteProducts"], 1);
    assert_eq!(source["htmlLanguageScope"], "parserCallsThisAttempt");
    assert_eq!(
        source["reasonScope"],
        "checkedJournalAndAssemblyObservationsThisAttempt"
    );
}

#[test]
fn five_language_projection_retains_only_fixed_bounded_numeric_counters() {
    let secret = "PRIVATE_CUSTOM_LOCALE_COOKIE_123456789012345678";
    let keys = [
        "htmlJa",
        "htmlEn",
        "htmlKo",
        "htmlZhCn",
        "htmlZhTw",
        "htmlOther",
        "htmlUnknown",
    ];
    let mut row = json!({"parserCalls":28,"htmlLanguage":secret,"rawLocale":secret,(secret):9});
    for (index, key) in keys.iter().enumerate() {
        row[*key] = json!(index + 1);
    }
    let mut job = json!({"type":"boothSync","supportDiagnostics":{"version":1,
            "sources":{"purchase":row.clone()},"financial":row}});
    let value = parsed(&job, &[], &[]);
    let source = &value["supportDiagnostics"]["sources"]["purchase"];
    let financial = &value["supportDiagnostics"]["financial"];
    assert_eq!(source["parserCalls"], 28);
    assert_eq!(source["htmlLanguageScope"], "parserCallsThisAttempt");
    assert_eq!(financial["scope"], "parserCallsThisAttempt");
    assert_eq!(source.as_object().unwrap().len(), 9);
    assert_eq!(financial.as_object().unwrap().len(), 8);
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(source[*key], index + 1, "source {key}");
        assert_eq!(financial[*key], index + 1, "financial {key}");
    }
    assert!(!report(&job, &[], &[]).contains(secret));
    for invalid in [
        json!(secret),
        json!("1"),
        json!(-1),
        json!(1.5),
        json!(true),
        json!({"locale":secret}),
        json!([1]),
        json!(MAX_COUNT + 1),
        json!(123456789012345678u64),
    ] {
        for key in keys {
            job["supportDiagnostics"]["sources"]["purchase"][key] = invalid.clone();
            job["supportDiagnostics"]["financial"][key] = invalid.clone();
        }
        let value = parsed(&job, &[], &[]);
        for key in keys {
            assert!(
                value["supportDiagnostics"]["sources"]["purchase"][key].is_null(),
                "source {key}"
            );
            assert!(
                value["supportDiagnostics"]["financial"][key].is_null(),
                "financial {key}"
            );
        }
        let text = report(&job, &[], &[]);
        assert!(!text.contains(secret));
        assert!(!text.contains("123456789012345678"));
    }
}

#[test]
fn retry_removes_previous_attempt_facts_without_changing_business_checkpoints() {
    let mut job = json!({"id":"PRIVATE_JOB","type":"gumroadSync","args":{"mode":"incremental"},
            "supportDiagnostics":{"version":1},"supportLastStage":"enriching","requestMetrics":{"requests":99},
            "progress":{"stage":"needsAction","pages":5,"loginStage":"cookieAccess","lastErrorCode":"AUTH_REQUIRED"}});
    reset_attempt(&mut job);
    for key in ["supportDiagnostics", "supportLastStage", "requestMetrics"] {
        assert!(job.get(key).is_none());
    }
    assert_eq!(job["id"], "PRIVATE_JOB");
    assert_eq!(job["args"]["mode"], "incremental");
    assert_eq!(job["progress"], json!({"stage":"needsAction","pages":5}));
    assert_eq!(operation_stage("needsAction"), "unknown");
    assert_eq!(operation_stage("PRIVATE_STAGE"), "unknown");
    assert_eq!(operation_stage("cookieAccess"), "cookieAccess");
    let value = parsed(&job, &[], &[]);
    assert_eq!(value["captureAvailable"], false);
    assert!(value["job"].get("requestMetrics").is_none());
}

fn parsed(job: &Value, coverage: &[Value], checkpoints: &[Value]) -> Value {
    serde_json::from_str(&report(job, coverage, checkpoints)).unwrap()
}

#[test]
fn old_jobs_do_not_invent_captured_stage_requests_or_financial_failures() {
    let job = json!({"id":"PRIVATE_JOB","type":"boothSync","state":"needsAction","errorCode":"SYNC_INCOMPLETE","progress":{"stage":"needsAction","pages":5}});
    let value = parsed(
        &job,
        &[json!({"source":"library","status":"incomplete","coverageComplete":false,"pages":5})],
        &[],
    );
    assert_eq!(value["captureAvailable"], false);
    assert!(value.get("supportDiagnostics").is_none());
    assert_eq!(value["job"]["errorCode"], "SYNC_INCOMPLETE");
    assert_eq!(value["sourceCoverage"]["library"]["pages"], 5);
    assert_eq!(
        value["sourceCoverage"]["library"]["coverageComplete"],
        false
    );
    assert!(value["sourceCoverage"].get("purchase").is_none());
    assert!(!report(&job, &[], &[]).contains("PRIVATE_JOB"));
}

#[test]
fn captured_facts_are_reprojected_without_private_strings_or_dynamic_keys() {
    let secret = "PRIVATE_MARKER_COOKIE_ORDER_TITLE_PATH_123456789";
    let job = json!({
        "id":secret,"accountId":secret,"accountName":secret,"type":secret,"store":secret,
        "state":secret,"errorCode":secret,"stage":secret,"createdAt":secret,
        "args":{"url":format!("https://example.invalid/?cookie={secret}")},
        "failures":{(secret):{"errorCode":secret,"fileName":secret,"path":secret}},
        "progress":{"stage":secret,"source":secret,"pages":secret,"private":secret},
        "supportDiagnostics":{"version":1,"lastStage":secret,"rawError":secret,
            "sources":{"library":{"state":secret,"errorCode":secret,"rows":3,"scansStarted":2,"scansComplete":1,"scansIncomplete":1,"scansFailed":0,"terminal":true,"title":secret},(secret):{"rows":9}},
            "http":{"receipt":{"requests":2,"lastStatus":403,"cookie":secret,"url":secret},(secret):{"requests":9}},
            "financial":{"rows":3,"rowsWithPaidProof":2,"paidAmount":991,"acquiredAt":secret,"scope":secret}
        }
    });
    let rows = vec![
        json!({"source":"library","status":secret,"errorCode":secret,"pages":2,"accountId":secret,"route":secret}),
        json!({"source":secret,"pages":1}),
    ];
    let text = report(&job, &rows, &rows);
    assert!(!text.contains(secret));
    for forbidden in [
        "rawError",
        "accountId",
        "accountName",
        "fileName",
        "paidAmount",
        "acquiredAt",
        "https:",
        "createdAt",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["job"]["errorCode"], "UNKNOWN_ERROR");
    assert_eq!(value["job"]["type"], "unknown");
    assert_eq!(value["job"]["failures"]["byCode"]["UNKNOWN_ERROR"], 1);
    assert_eq!(value["supportDiagnostics"]["sources"]["library"]["rows"], 3);
    assert_eq!(
        value["supportDiagnostics"]["sources"]["library"]["scansStarted"],
        2
    );
    assert_eq!(
        value["supportDiagnostics"]["http"]["receipt"]["lastStatus"],
        403
    );
    assert_eq!(
        value["supportDiagnostics"]["financial"]["scope"],
        "parserCallsThisAttempt"
    );
    assert_eq!(
        value["supportDiagnostics"]["financial"]["rowsWithPaidProof"],
        2
    );
    assert_eq!(value["sourceCoverage"].as_object().unwrap().len(), 1);
}

#[test]
fn codes_and_stages_require_exact_static_membership_even_for_code_like_secrets() {
    for value in [
        "SECRET_COOKIE",
        "ORDER_123456789",
        "AUTH_REQUIRED:PRIVATE_TOKEN",
        "AUTH_REQUIRED\nPRIVATE",
        "UNITY_PRIVATE_TITLE",
        "ai_PRIVATE_KEY",
        "AUTH_REQUIRED\u{0}",
    ] {
        assert_eq!(safe_code(value), "UNKNOWN_ERROR");
        assert_eq!(safe_stage(value), "unknown");
    }
    assert_eq!(
        safe_code("FREE_ITEM_ROLE_UNPROVEN"),
        "FREE_ITEM_ROLE_UNPROVEN"
    );
    assert_eq!(safe_code("NETWORK_TIMEOUT"), "NETWORK_TIMEOUT");
    assert_eq!(
        safe_code("PUBLIC_METADATA_INVALID"),
        "PUBLIC_METADATA_INVALID"
    );
    assert_eq!(safe_code("archive_unsafe_link"), "archive_unsafe_link");
    assert_eq!(safe_stage("publishing"), "publishing");
}

#[test]
fn last_operation_stage_and_interrupted_trace_are_safe_without_inferred_sync_modes() {
    let job = json!({"type":"boothSync","state":"failed","errorCode":"NETWORK_TIMEOUT",
            "supportLastStage":"enriching","args":{"mode":"full","accountId":"PRIVATE_ACCOUNT"},
            "supportDiagnostics":{"version":1,"sources":{"purchase":{"state":"interrupted","errorCode":"NETWORK_TIMEOUT","rows":2}}}});
    let value = parsed(&job, &[], &[]);
    assert_eq!(value["job"]["supportLastStage"], "enriching");
    assert_eq!(value["job"]["mode"], "full");
    assert_eq!(value["job"]["errorCode"], "NETWORK_TIMEOUT");
    assert_eq!(
        value["supportDiagnostics"]["sources"]["purchase"]["state"],
        "interrupted"
    );
    for invalid in [
        json!("PRIVATE_STAGE"),
        json!({"value":"PRIVATE_STAGE"}),
        Value::Null,
    ] {
        let mut poisoned = job.clone();
        poisoned["supportLastStage"] = invalid;
        assert_eq!(
            parsed(&poisoned, &[], &[])["job"]["supportLastStage"],
            "unknown"
        );
        assert!(!report(&poisoned, &[], &[]).contains("PRIVATE"));
    }
    for mode in [
        json!("update"),
        json!("PRIVATE_MODE"),
        json!(true),
        Value::Null,
    ] {
        let mut changed = job.clone();
        changed["args"]["mode"] = mode;
        assert!(parsed(&changed, &[], &[])["job"].get("mode").is_none());
    }
    let incremental = json!({"type":"gumroadSync","args":{"mode":"incremental"}});
    assert_eq!(parsed(&incremental, &[], &[])["job"]["mode"], "incremental");
    assert_eq!(
        parsed(
            &json!({"type":"boothSync","args":{"mode":"forced"}}),
            &[],
            &[]
        )["job"]["mode"],
        "forced"
    );
    assert!(
        parsed(&json!({"type":"download","args":{"mode":"full"}}), &[], &[])["job"]
            .get("mode")
            .is_none()
    );
    assert!(parsed(&json!({"type":"boothSync"}), &[], &[])["job"]
        .get("mode")
        .is_none());
}

#[test]
fn elapsed_backoff_and_provider_metrics_keep_only_typed_fixed_operational_facts() {
    let secret = "PRIVATE_METRICS_COOKIE_ORDER_123456789012345678";
    let mut job = json!({"type":"gumroadSync","requestMetrics":{
            "scope":"providerCollectionInterval","requests":7,"pacingWaitMs":23,
            "responseWaitMs":49,"activeRequestLimit":2,"startSpacingMs":500,"private":secret,
            "byKind":{"content":{"requests":3,"bodyReadMs":8,"retries":1,"error":secret},
                "public":{"requests":4,"rateLimits":1,"serverErrors":2},(secret):{"requests":9}}
            },"supportDiagnostics":{"version":1,"elapsedMs":450,"lastStage":"cookieAccess",
                "http":{"receipt":{"backoffs":2,"backoffMs":20,"url":secret}}}});
    let value = parsed(&job, &[], &[]);
    assert_eq!(value["supportDiagnostics"]["elapsedMs"], 450);
    assert_eq!(value["supportDiagnostics"]["lastStage"], "cookieAccess");
    assert_eq!(
        value["supportDiagnostics"]["http"]["receipt"]["backoffs"],
        2
    );
    assert_eq!(
        value["supportDiagnostics"]["http"]["receipt"]["backoffMs"],
        20
    );
    assert_eq!(
        value["job"]["requestMetrics"]["scope"],
        "providerCollectionInterval"
    );
    assert_eq!(
        value["job"]["requestMetrics"]["byKind"]["content"]["requests"],
        3
    );
    assert_eq!(
        value["job"]["requestMetrics"]["byKind"]["public"]["serverErrors"],
        2
    );
    assert_eq!(
        value["job"]["requestMetrics"]["byKind"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert!(!report(&job, &[], &[]).contains(secret));
    for invalid in [
        json!(secret),
        json!("50"),
        json!(-1),
        json!(2.5),
        json!(false),
        json!(MAX_COUNT + 1),
        json!({"secret":secret}),
    ] {
        job["supportDiagnostics"]["elapsedMs"] = invalid.clone();
        job["supportDiagnostics"]["http"]["receipt"]["backoffMs"] = invalid.clone();
        job["requestMetrics"]["pacingWaitMs"] = invalid.clone();
        job["requestMetrics"]["byKind"]["content"]["requests"] = invalid;
        let projected = parsed(&job, &[], &[]);
        assert!(projected["supportDiagnostics"]["elapsedMs"].is_null());
        assert!(projected["supportDiagnostics"]["http"]["receipt"]["backoffMs"].is_null());
        assert!(projected["job"]["requestMetrics"]["pacingWaitMs"].is_null());
        assert!(projected["job"]["requestMetrics"]["byKind"]["content"]["requests"].is_null());
        assert!(!report(&job, &[], &[]).contains(secret));
    }
    job["type"] = json!("gumroadUpdate");
    assert!(parsed(&job, &[], &[])["job"]
        .get("requestMetrics")
        .is_some());
    job["requestMetrics"]["scope"] = json!(secret);
    assert!(parsed(&job, &[], &[])["job"]
        .get("requestMetrics")
        .is_none());
    job["requestMetrics"]["scope"] = json!("providerCollectionInterval");
    job["type"] = json!("boothSync");
    assert!(parsed(&job, &[], &[])["job"]
        .get("requestMetrics")
        .is_none());
}

#[test]
fn counters_reject_ids_wrong_types_and_non_http_statuses_without_coercion() {
    for invalid in [
        json!("PRIVATE_NUMERIC_SECRET"),
        json!("17"),
        json!(-1),
        json!(1.5),
        json!(true),
        json!({"token":"PRIVATE_NESTED"}),
        json!([3]),
        json!(MAX_COUNT + 1),
        json!(123456789012345678u64),
    ] {
        let job = json!({"type":"boothSync","state":"failed","supportDiagnostics":{"version":1,"sources":{"free":{"rows":invalid.clone(),"terminal":invalid.clone()}},"http":{"public":{"requests":invalid.clone(),"lastStatus":invalid.clone()}},"financial":{"proofRejected":invalid.clone()}}});
        let value = parsed(
            &job,
            &[json!({"source":"free","pages":invalid.clone()})],
            &[],
        );
        assert!(value["supportDiagnostics"]["sources"]["free"]["rows"].is_null());
        assert_eq!(
            value["supportDiagnostics"]["sources"]["free"]["terminal"],
            invalid.as_bool().map(Value::from).unwrap_or(Value::Null)
        );
        assert!(value["supportDiagnostics"]["http"]["public"]["requests"].is_null());
        assert!(value["supportDiagnostics"]["http"]["public"]["lastStatus"].is_null());
        assert!(value["supportDiagnostics"]["financial"]["proofRejected"].is_null());
        assert!(value["sourceCoverage"]["free"]["pages"].is_null());
        assert!(!report(&job, &[], &[]).contains("PRIVATE"));
    }
    let value = parsed(
        &json!({"supportDiagnostics":{"version":1,"http":{"other":{"requests":0,"lastStatus":600}},"financial":{"rows":MAX_COUNT}}}),
        &[],
        &[],
    );
    assert_eq!(value["supportDiagnostics"]["http"]["other"]["requests"], 0);
    assert!(value["supportDiagnostics"]["http"]["other"]["lastStatus"].is_null());
    assert_eq!(value["supportDiagnostics"]["financial"]["rows"], MAX_COUNT);
}

#[test]
fn unsupported_trace_versions_remain_unavailable_and_fixed_source_rows_are_bounded() {
    for version in [json!(2), json!("1"), json!(true), Value::Null] {
        let value = parsed(
            &json!({"supportDiagnostics":{"version":version,"lastStage":"publishing","financial":{"proofRejected":99}}}),
            &[],
            &[],
        );
        assert_eq!(value["captureAvailable"], false);
        assert!(value.get("supportDiagnostics").is_none());
    }
    let records: Vec<_> = (0..1000)
        .map(|index| json!({"source":"library","pages":index,"private":"PRIVATE_DUPLICATE"}))
        .collect();
    let value = parsed(&json!({}), &records, &records);
    assert_eq!(value["sourceCoverage"].as_object().unwrap().len(), 1);
    assert_eq!(value["checkpoints"].as_object().unwrap().len(), 1);
    assert_eq!(value["checkpoints"]["library"]["pages"], 0);
}

#[test]
fn maximal_known_report_is_valid_bounded_json_and_preserves_operational_facts() {
    let max_fields = |keys: &[&str]| -> Value {
        Value::Object(
            keys.iter()
                .map(|key| ((*key).into(), json!(MAX_COUNT)))
                .collect(),
        )
    };
    let mut capture = json!({"version":1,"lastStage":"enriching","sources":{},"http":{},"financial":max_fields(FINANCIAL_COUNTS)});
    for source in TRACE_SOURCES {
        let mut row = max_fields(&[
            "pages",
            "details",
            "rows",
            "unresolved",
            "scansStarted",
            "scansComplete",
            "scansIncomplete",
            "scansFailed",
        ]);
        row["state"] = json!("incomplete");
        row["errorCode"] = json!("FREE_ITEM_CONFLICTING_ENTRIES");
        row["terminal"] = json!(true);
        row["reasons"] = [
            "SOURCE_HEAD_CHANGED",
            "SOURCE_TERMINAL_UNPROVEN",
            "SOURCE_MEMBERSHIP_UNBOUND",
            "SOURCE_MANIFEST_INCOMPLETE",
            "DIGITAL_CONTROLS_UNRESOLVED",
            "PRODUCT_ID_UNRESOLVED",
            "PRODUCT_TITLE_UNRESOLVED",
            "FREE_ITEM_ENTRY_UNPROVEN",
            "FREE_ITEM_FILENAME_UNPROVEN",
            "FREE_ITEM_HISTORY_UNPROVEN",
            "FREE_ITEM_IDENTITY_UNPROVEN",
            "FREE_ITEM_COUNT_UNPROVEN",
            "FREE_ITEM_UNKNOWN_DROPDOWN",
            "FREE_ITEM_ORPHAN_CONTROL",
            "FREE_ITEM_ROLE_UNPROVEN",
            "FREE_ITEM_CONFLICTING_ENTRIES",
        ]
        .iter()
        .map(|key| ((*key).into(), json!(MAX_COUNT)))
        .collect::<Map<String, Value>>()
        .into();
        row["lastClosure"] = json!({"terminalSeen":true,"pendingRequests":MAX_COUNT,"unresolvedRows":MAX_COUNT,
                "enrichmentPending":MAX_COUNT,"incompleteProducts":MAX_COUNT,"membershipUnbound":true,"headStable":false});
        row["parserCalls"] = json!(MAX_COUNT);
        for language in [
            "htmlJa",
            "htmlEn",
            "htmlKo",
            "htmlZhCn",
            "htmlZhTw",
            "htmlOther",
            "htmlUnknown",
        ] {
            row[language] = json!(MAX_COUNT);
        }
        capture["sources"][*source] = row;
    }
    for scope in HTTP_SCOPES {
        let mut row = max_fields(HTTP_COUNTS);
        row["lastStatus"] = json!(599);
        capture["http"][*scope] = row;
    }
    let mut rows = Vec::new();
    for source in SOURCES {
        let mut row = max_fields(COVERAGE_COUNTS);
        row["source"] = json!(source);
        row["detailsCompleted"] = json!(MAX_COUNT);
        row["unresolved"] = json!(MAX_COUNT);
        row["state"] = json!("incomplete");
        row["errorCode"] = json!("GUMROAD_PUBLIC_METADATA_UNAVAILABLE");
        for flag in COVERAGE_FLAGS
            .iter()
            .copied()
            .chain(["terminal", "complete", "finished"])
        {
            row[flag] = json!(true);
        }
        rows.push(row);
    }
    let job = json!({"type":"boothSync","state":"needsAction","errorCode":"SYNC_INCOMPLETE","supportDiagnostics":capture,"progress":max_fields(PROGRESS_COUNTS)});
    let text = report(&job, &rows, &rows);
    assert!(text.len() <= MAX_REPORT_BYTES, "{}", text.len());
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["captureAvailable"], true);
    assert_eq!(value["supportDiagnostics"]["lastStage"], "enriching");
    assert_eq!(value["supportDiagnostics"]["financial"]["rows"], MAX_COUNT);
    assert_eq!(
        value["supportDiagnostics"]["http"]["receipt"]["requests"],
        MAX_COUNT
    );
    assert_eq!(
        value["supportDiagnostics"]["sources"]["library"]["reasons"]["SOURCE_HEAD_CHANGED"],
        MAX_COUNT
    );
}
