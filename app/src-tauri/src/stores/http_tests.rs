
use super::*;
#[tokio::test]
async fn resolver_denial_does_not_claim_account_expiry_without_login_evidence() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (status, location, resolver, expected) in [
        (
            "401 Unauthorized",
            None,
            true,
            Some("DOWNLOAD_RESOLVER_FAILED"),
        ),
        ("401 Unauthorized", None, false, Some("AUTH_REQUIRED")),
        (
            "302 Found",
            Some("https://gumroad.com/login?next=%2Flibrary"),
            true,
            Some("AUTH_REQUIRED"),
        ),
        (
            "302 Found",
            Some("https://gumroad.com/two-factor"),
            true,
            Some("AUTH_REQUIRED"),
        ),
        (
            "302 Found",
            Some("https://gumroad.com.evil.invalid/login"),
            true,
            None,
        ),
        ("302 Found", Some("http://gumroad.com/login"), true, None),
        (
            "302 Found",
            Some("https://files.gumroad.com/login"),
            true,
            None,
        ),
        (
            "302 Found",
            Some("https://gumroad.com/r/authored/product_files"),
            true,
            None,
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\n{}Content-Length: 0\r\nConnection: close\r\n\r\n",
            location
                .map(|value| format!("Location: {value}\r\n"))
                .unwrap_or_default()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let _request = socket.read(&mut bytes).await.unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut transport = MetadataHttp::new(StoreProvider::Gumroad);
        transport.authored_origin = Some(origin.clone());
        let result = transport
            .request(
                &client,
                origin,
                resolver,
                if resolver { Method::HEAD } else { Method::GET },
                None,
                None,
            )
            .await;
        match expected {
            Some(code) => assert_eq!(result.unwrap_err().to_string(), code),
            None => assert_eq!(result.unwrap().status(), StatusCode::FOUND),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn a_body_cut_off_after_its_headers_is_retried() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        for response in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{\"".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".as_slice(),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let _request = socket.read(&mut bytes).await.unwrap();
            socket.write_all(response).await.unwrap();
        }
    });
    let mut transport = MetadataHttp::new(StoreProvider::Gumroad);
    transport.authored_origin = Some(origin.clone());
    let client = Client::builder().no_proxy().build().unwrap();
    let (status, body) = transport
        .get_classified(&client, origin, RequestKind::Public)
        .await
        .unwrap();
    assert_eq!((status, body.as_str()), (StatusCode::OK, "{}"));
    server.await.unwrap();
    let metrics = transport.classified_since([[0; 13]; 2]);
    assert_eq!(metrics["public"]["requests"], 2);
    assert_eq!(metrics["public"]["retries"], 1);
    assert_eq!(metrics["public"]["transportErrors"], 1);
}
#[test]
fn retry_after_rejects_unrepresentable_delays() {
    assert_eq!(
        retry_after(Some("18446744073709551615")),
        Duration::from_secs(5)
    );
    assert_eq!(retry_after(Some("600")), Duration::from_secs(600));
    assert_eq!(retry_after(Some("bad")), Duration::from_secs(5));
    assert_eq!(retry_after(None), Duration::from_secs(5));
}
#[tokio::test]
async fn terminal_rate_limit_defers_other_requests_without_waiting_to_return() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let _request = socket.read(&mut bytes).await.unwrap();
        socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let mut transport = MetadataHttp::new(StoreProvider::Gumroad);
    transport.authored_origin = Some(origin.clone());
    let began = Instant::now();
    let error = transport
        .get(&Client::builder().no_proxy().build().unwrap(), origin)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "RATE_LIMITED");
    assert!(began.elapsed() < Duration::from_secs(5));
    assert!(
        transport
            .pace
            .lock()
            .await
            .saturating_duration_since(Instant::now())
            >= Duration::from_secs(599)
    );
    server.await.unwrap();
}

#[tokio::test]
async fn authored_gumroad_http_measures_serial_and_full_product_pipeline() {
    use futures_util::StreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut bytes = [0; 4096];
                let _request = socket.read(&mut bytes).await.unwrap();
                tokio::time::sleep(Duration::from_millis(3750)).await;
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    )
                    .await
                    .unwrap();
            });
        }
    });
    let client = Client::builder().no_proxy().build().unwrap();
    let mut measurements = Vec::new();
    for parallel in [false, true] {
        let mut transport = MetadataHttp::new(StoreProvider::Gumroad);
        transport.authored_origin = Some(origin.clone());
        if !parallel {
            transport.metadata = Arc::new(Semaphore::new(1));
        }
        let began = Instant::now();
        if parallel {
            let mut products = futures_util::stream::iter(0..4)
                .map(|id| {
                    let transport = &transport;
                    let client = &client;
                    let origin = &origin;
                    async move {
                        transport
                            .get_classified(
                                client,
                                origin.join(&format!("content/{id}")).unwrap(),
                                RequestKind::Content,
                            )
                            .await
                            .unwrap();
                        transport
                            .get_classified(
                                client,
                                origin.join(&format!("public/{id}")).unwrap(),
                                RequestKind::Public,
                            )
                            .await
                            .unwrap();
                        id
                    }
                })
                .buffered(2);
            while products.next().await.is_some() {}
        } else {
            for id in 0..4 {
                transport
                    .get_classified(
                        &client,
                        origin.join(&format!("content/{id}")).unwrap(),
                        RequestKind::Content,
                    )
                    .await
                    .unwrap();
                transport
                    .get_classified(
                        &client,
                        origin.join(&format!("public/{id}")).unwrap(),
                        RequestKind::Public,
                    )
                    .await
                    .unwrap();
            }
        }
        let metrics = transport.metrics();
        let by_kind = transport.classified_since([[0; 13]; 2]);
        assert_eq!(by_kind["content"]["requests"], 4);
        assert_eq!(by_kind["public"]["requests"], 4);
        assert_eq!(by_kind["content"]["retries"], 0);
        assert_eq!(by_kind["public"]["retries"], 0);
        measurements.push(serde_json::json!({"parallel":parallel,"fullProductPipeline":parallel,"products":4,"requests":metrics[0],"requestsPerProduct":metrics[0]/4,"wallMs":began.elapsed().as_millis(),"pacingWaitMs":metrics[1],"responseWaitMs":metrics[2],"byKind":by_kind,"startSpacingMs":transport.start_spacing_ms(),"authoredResponseDelayMs":3750}));
        assert_eq!(metrics[0], 8);
    }
    server.abort();
    assert!(
        measurements[1]["wallMs"].as_u64().unwrap() < measurements[0]["wallMs"].as_u64().unwrap()
    );
    if let Ok(path) = std::env::var("NNA_STORE_EVIDENCE_DIR") {
        std::fs::write(
            std::path::Path::new(&path).join("gumroad-full-pipeline-timing.json"),
            serde_json::to_vec_pretty(&measurements).unwrap(),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn authored_public_retry_counts_distinguish_default_backoff_from_response_latency() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        for attempt in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let _request = socket.read(&mut bytes).await.unwrap();
            let response = if attempt < 2 {
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".as_slice()
            } else {
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".as_slice()
            };
            socket.write_all(response).await.unwrap();
        }
    });
    let client = Client::builder().no_proxy().build().unwrap();
    let mut transport = MetadataHttp::new(StoreProvider::Gumroad);
    transport.authored_origin = Some(origin.clone());
    let began = Instant::now();
    transport
        .get_classified(&client, origin, RequestKind::Public)
        .await
        .unwrap();
    server.await.unwrap();
    let metrics = transport.classified_since([[0; 13]; 2]);
    assert_eq!(metrics["public"]["requests"], 3);
    assert_eq!(metrics["public"]["retries"], 2);
    assert_eq!(metrics["public"]["serverErrors"], 2);
    assert_eq!(metrics["content"]["requests"], 0);
    assert_eq!(retry_after(None), Duration::from_secs(5));
    if let Ok(path) = std::env::var("NNA_STORE_EVIDENCE_DIR") {
        std::fs::write(std::path::Path::new(&path).join("gumroad-retry-counts.json"),serde_json::to_vec_pretty(&serde_json::json!({"wallMs":began.elapsed().as_millis(),"byKind":metrics,"authoredResponses":[503,503,200],"retryAfterHeader":false})).unwrap()).unwrap();
    }
}
#[test]
fn metadata_transport_rejects_other_provider_and_lookalike_origins() {
    let url = |value| Url::parse(value).unwrap();
    assert!(metadata_destination(
        StoreProvider::Gumroad,
        &url("https://authored.gumroad.com/l/fixture.json")
    ));
    for host in super::super::session::JINXXY_HOSTS
        .iter()
        .copied()
        .chain(["other.jinxxy.com", "api.jinxxy.com"])
    {
        assert!(metadata_destination(
            StoreProvider::Jinxxy,
            &Url::parse(&format!("https://{host}/my/inventory")).unwrap()
        ));
    }
    assert_eq!(
        jinxxy_graphql_url(&url("https://zh.jinxxy.com/")).unwrap(),
        url("https://zh.jinxxy.com/graphql")
    );
    for value in [
        "https://evil-jinxxy.com/",
        "https://gumroad.com/",
        "https://zh.jinxxy.com/my/inventory",
        "https://zh.jinxxy.com/?redirect=authored",
    ] {
        assert!(jinxxy_graphql_url(&url(value)).is_err());
    }
    for value in [
        "https://zh.jinxxy.com.evil.test/my/inventory",
        "https://evil-jinxxy.com/my/inventory",
        "http://zh.jinxxy.com/login",
        "https://secret@zh.jinxxy.com/login",
        "https://zh.jinxxy.com:8443/login",
    ] {
        assert!(!metadata_destination(StoreProvider::Jinxxy, &url(value)));
    }
    assert!(!metadata_destination(
        StoreProvider::Gumroad,
        &url("https://zh.jinxxy.com/my/inventory")
    ));
    for value in [
        "https://jinxxy.com/my/lists",
        "https://gumroad.com.evil.test/library",
        "https://gumroad.com:8443/library",
        "https://secret@gumroad.com/library",
    ] {
        assert!(!metadata_destination(StoreProvider::Gumroad, &url(value)));
    }
}
#[test]
fn redirects_and_files_reach_any_public_https_host_but_not_local_or_sign_in_pages() {
    let url = |value| Url::parse(value).unwrap();
    for value in [
            "https://files.gumroad.com/authored/file.html?authored-capability=value",
            "https://jinxxy-products.62bb56dad0a3633d04ee066263e8507d.r2.cloudflarestorage.com/authored.zip?authored-signature=value",
            "https://authored-cdn.example/",
            "https://authored-cdn.example:8443/authored.zip",
            "https://8.8.8.8/authored.zip",
            "https://[2606:4700::1]/authored.zip",
        ] {
            assert!(public_https(&url(value)), "{value}");
            assert!(gumroad_file_destination(&url(value)), "{value}");
            assert!(download_destination(StoreProvider::Jinxxy, &url(value)), "{value}");
        }
    for value in [
        "http://files.gumroad.com/authored.zip",
        "ftp://authored-cdn.example/authored.zip",
        "https://secret@files.gumroad.com/authored.zip",
        "https://user:secret@authored-cdn.example/authored.zip",
        "https://localhost/authored.zip",
        "https://authored.localhost/authored.zip",
        "https://nas.local/authored.zip",
        "https://metadata.internal/authored.zip",
        "https://intranet/authored.zip",
        "https://127.0.0.1/authored.zip",
        "https://10.0.0.1/authored.zip",
        "https://192.168.1.1/authored.zip",
        "https://169.254.169.254/authored.zip",
        "https://[::1]/authored.zip",
        "https://[fc00::1]/authored.zip",
        "https://[::ffff:127.0.0.1]/authored.zip",
    ] {
        assert!(!public_https(&url(value)), "{value}");
        assert!(!gumroad_file_destination(&url(value)), "{value}");
        assert!(
            !download_destination(StoreProvider::Jinxxy, &url(value)),
            "{value}"
        );
    }
    // A redirect may pass through any public page, but a file is never the store's sign-in.
    assert!(public_https(&url("https://gumroad.com/login")));
    assert!(!gumroad_file_destination(&url(
        "https://gumroad.com/login?next=%2Flibrary"
    )));
    assert!(!download_destination(
        StoreProvider::Jinxxy,
        &url("https://jinxxy.com/login?redirect=%2Fmy%2Finventory")
    ));
    assert!(!download_destination(
        StoreProvider::Jinxxy,
        &url("https://zh.jinxxy.com/sign_in")
    ));
}
