use super::*;
#[tokio::test]
async fn booth_five_languages_keep_distinct_closed_counters_without_raw_locale_values() {
    let languages = [
        ("ja", "htmlJa"),
        ("en", "htmlEn"),
        ("ko", "htmlKo"),
        ("zh-cn", "htmlZhCn"),
        ("zh-tw", "htmlZhTw"),
        ("other", "htmlOther"),
        ("unknown", "htmlUnknown"),
    ];
    let ((), facts) = capture(async {
        for (language, _) in languages {
            parser_language("purchase", language);
            receipt(&json!({}), 1, 0, 0, false, language);
        }
        // Only the parser classifies raw locale tags. The recorder never
        // interprets or retains caller-supplied tags or private strings.
        for invalid in [
            "zh-Hant-PRIVATE_LANGUAGE",
            "ko-PRIVATE_COOKIE",
            "EN",
            "htmlZhTw",
            "",
        ] {
            parser_language("purchase", invalid);
            receipt(&json!({}), 0, 0, 0, false, invalid);
        }
    })
    .await;
    let source = &facts["sources"]["purchase"];
    let financial = &facts["financial"];
    assert_eq!(source["parserCalls"], 12);
    assert_eq!(financial["receiptPages"], 12);
    for (_, key) in languages {
        let expected = if key == "htmlUnknown" { 6 } else { 1 };
        assert_eq!(source[key], expected, "source {key}");
        assert_eq!(financial[key], expected, "financial {key}");
    }
    assert_eq!(source.as_object().unwrap().len(), 8);
    let text = facts.to_string();
    for forbidden in [
        "PRIVATE",
        "zh-Hant",
        "ko-PRIVATE",
        "\"zh-cn\"",
        "\"zh-tw\"",
        "\"EN\"",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
}
#[tokio::test]
async fn attempts_are_isolated_and_do_not_retain_url_or_unknown_strings() {
    let ((), facts) = capture(async {
        let url =
            url::Url::parse("https://accounts.booth.pm/orders/123456789?cookie=PRIVATE_COOKIE")
                .unwrap();
        source_start("purchase");
        stage("PRIVATE_STAGE");
        stage("collecting");
        request(&url, 0);
        response(&url, 503);
        request(&url, 1);
        transport(&url, "timeout");
        transport(&url, "tls");
        backoff(&url, std::time::Duration::from_secs(2));
        page("purchase", true, 2, 1, false);
        source_done("purchase", None, Some("PRIVATE_ERROR"));
        receipt(
            &json!({"unitPriceItems":1,"private":"PRIVATE_COOKIE"}),
            2,
            1,
            0,
            false,
            "PRIVATE_LANGUAGE",
        );
    })
    .await;
    assert_eq!(facts["lastStage"], "collecting");
    assert_eq!(facts["http"]["receipt"]["requests"], 2);
    assert_eq!(facts["http"]["receipt"]["retries"], 1);
    assert_eq!(facts["http"]["receipt"]["timeouts"], 1);
    assert_eq!(facts["http"]["receipt"]["tlsFailures"], 1);
    assert_eq!(facts["http"]["receipt"]["transportFailures"], 2);
    assert_eq!(facts["http"]["receipt"]["backoffMs"], 2000);
    assert_eq!(facts["sources"]["purchase"]["errorCode"], "UNKNOWN_ERROR");
    assert_eq!(facts["financial"]["htmlUnknown"], 1);
    let text = facts.to_string();
    for private in ["PRIVATE", "123456789", "https://", "orders/"] {
        assert!(!text.contains(private));
    }
    let (_, next) = capture(async {}).await;
    assert_eq!(next["http"], json!({}));
    assert!(snapshot().is_none());
}
#[tokio::test]
async fn simultaneous_attempts_and_dropped_sources_keep_separate_operational_facts() {
    let first = capture(async {
        source_start("purchase");
        source_start("gift");
        tokio::task::yield_now().await;
        page("purchase", true, 3, 1, false);
        source_done("purchase", Some(false), None);
        source_start("purchase");
        page("purchase", true, 2, 0, true);
        source_done("purchase", Some(true), None);
    });
    let second = capture(async {
        source_start("library");
        tokio::task::yield_now().await;
        page("library", false, 2, 0, true);
        source_done("library", Some(true), None);
    });
    let (((), first), ((), second)) = tokio::join!(first, second);
    assert_eq!(first["sources"]["purchase"]["rows"], 5);
    assert_eq!(first["sources"]["purchase"]["scansStarted"], 2);
    assert_eq!(first["sources"]["purchase"]["scansComplete"], 1);
    assert_eq!(first["sources"]["purchase"]["scansIncomplete"], 1);
    assert_eq!(first["sources"]["purchase"]["state"], "incomplete");
    assert_eq!(first["sources"]["gift"]["state"], "interrupted");
    assert!(first["sources"].get("library").is_none());
    assert_eq!(second["sources"]["library"]["state"], "complete");
    assert_eq!(second["sources"]["library"]["rows"], 2);
    assert!(second["sources"].get("purchase").is_none());
    assert!(first["elapsedMs"].as_u64().is_some());
}
#[tokio::test]
async fn login_substages_are_recorded_as_fixed_operational_phases() {
    let ((), facts) = capture(async {
            job_stage(&json!({"type":"boothLogin","progress":{"stage":"collecting","loginStage":"cookieAccess"}}));
            assert_eq!(snapshot().unwrap()["lastStage"],"cookieAccess");
            job_stage(&json!({"type":"boothLogin","progress":{"stage":"collecting","loginStage":"parserRecognition"}}));
            job_stage(&json!({"type":"boothLogin","progress":{"stage":"failed","loginStage":"PRIVATE_STAGE"}}));
        }).await;
    assert_eq!(facts["lastStage"], "parserRecognition");
}
#[tokio::test]
async fn transport_failures_name_their_cause_from_local_authored_servers() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // Windows retries a refused loopback connect for about two seconds before failing.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let impatient = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
        .unwrap();
    let cause = |result: reqwest::Result<reqwest::Response>| network_cause(&result.unwrap_err());
    // Nothing listens on a port that was just released.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert_eq!(
        cause(
            client
                .get(format!("https://127.0.0.1:{port}/"))
                .send()
                .await
        ),
        "connect"
    );
    // A plain-text answer to a TLS handshake.
    let plain = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = plain.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = plain.accept().await.unwrap();
        let mut hello = [0u8; 512];
        let _ = socket.read(&mut hello).await;
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .await;
    });
    assert_eq!(
        cause(
            client
                .get(format!("https://127.0.0.1:{port}/"))
                .send()
                .await
        ),
        "tls"
    );
    // A server that accepts and never answers.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = silent.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (_socket, _) = silent.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    });
    assert_eq!(
        cause(
            impatient
                .get(format!("http://127.0.0.1:{port}/"))
                .send()
                .await
        ),
        "timeout"
    );
}
