#[test]
fn startup_recovery_isolates_orphan_jobs_and_keeps_later_jobs_resumable() {
    let path = std::env::temp_dir().join(format!("gumroad-startup-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let missing = "d0140000-0000-4000-8000-000000000001";
    let healthy = "d0140000-0000-4000-8000-000000000002";
    catalogue
        .put("account", healthy, json!({"id":healthy,"store":"Gumroad"}))
        .unwrap();
    for (id, owner) in [
        ("authored-orphan-job", missing),
        ("authored-healthy-job", healthy),
    ] {
        let job = json!({"id":id,"type":"gumroadSync","store":"Gumroad","accountId":owner,"state":"running"});
        catalogue.put("job", id, job.clone()).unwrap();
        let recovered = service.prepare_recovered_job(job).unwrap();
        assert_eq!(recovered.is_some(), owner == healthy);
    }
    let orphan = catalogue
        .get("job", "authored-orphan-job")
        .unwrap()
        .unwrap();
    assert_eq!(orphan["state"], "needsAction");
    assert_eq!(orphan["errorCode"], "ACCOUNT_UNAVAILABLE");
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn stale_retry_cannot_overwrite_the_first_retry_attempt() {
    let path = std::env::temp_dir().join(format!("gumroad-retry-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let previous = json!({"id":"authored-retry","type":"gumroadLogin","store":"Gumroad","accountId":"authored-account","state":"needsAction","errorCode":"RETRY_REQUIRED"});
    catalogue
        .put("job", "authored-retry", previous.clone())
        .unwrap();
    let mut running = service.requeue_job(&previous).unwrap();
    assert_eq!(running["state"], "queued");
    running["state"] = json!("running");
    running["progress"] = json!({"pages":5});
    catalogue
        .put("job", "authored-retry", running.clone())
        .unwrap();
    assert_eq!(
        safe_code(&service.requeue_job(&previous).unwrap_err()),
        "JOB_BUSY"
    );
    assert_eq!(
        catalogue.get("job", "authored-retry").unwrap().unwrap(),
        running
    );
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn content_pipeline_publishes_completed_grant_before_pending_work_and_merges_next_grant() {
    let path = std::env::temp_dir().join(format!("gumroad-page-pipeline-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let account = "authored-account";
    let job = json!({"id":"authored-content-job","state":"running","type":"gumroadSync","store":"Gumroad","accountId":account});
    catalogue
        .put("job", "authored-content-job", job.clone())
        .unwrap();
    let mut checkpoint = json!({"id":"authored-checkpoint","journalIds":[],"nextPages":[1,1],"publishedProducts":[],"complete":false});
    let mut first = content(
        "authored-product",
        "first-purchase",
        "first-file",
        "AUTHORED_CAPABILITY",
    );
    first.variant_id = Some("first-variant".into());
    let mut second = content(
        "authored-product",
        "second-purchase",
        "second-file",
        "AUTHORED_CAPABILITY",
    );
    second.variant_id = Some("second-variant".into());
    let (release, wait) = tokio::sync::oneshot::channel::<()>();
    let mut wait = Some(wait);
    let mut stream = futures_util::stream::iter([first, second].into_iter().enumerate().map(
        |(index, observation)| {
            let blocked = (index == 0).then(|| wait.take().unwrap());
            async move {
                if let Some(blocked) = blocked {
                    blocked.await.unwrap();
                }
                observation
            }
        },
    ))
    .buffer_unordered(2);
    let card = |observation: &ContentObservation| LibraryPurchase {
        purchase_id: observation.purchase_id.clone(),
        title: Some("Authored product".into()),
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: None,
        archived: false,
        content_route: None,
    };
    let ready = stream.next().await.unwrap();
    assert_eq!(ready.purchase_id, "second-purchase");
    let ids = service
        .consume_complete_observation(
            &job,
            account,
            &card(&ready),
            &ready,
            None,
            None,
            false,
            &mut checkpoint,
        )
        .unwrap();
    assert_eq!(ids, vec!["gumroad:authored-product"]);
    assert_eq!(checkpoint["nextPages"], json!([1, 1]));
    assert_eq!(checkpoint["complete"], false);
    assert_eq!(
        catalogue
            .related("acquisition", "gumroad:authored-product")
            .unwrap()
            .len(),
        1
    );
    release.send(()).unwrap();
    let ready = stream.next().await.unwrap();
    service
        .consume_complete_observation(
            &job,
            account,
            &card(&ready),
            &ready,
            None,
            None,
            false,
            &mut checkpoint,
        )
        .unwrap();
    assert_eq!(
        catalogue
            .related("acquisition", "gumroad:authored-product")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        catalogue
            .related("remote_file", "gumroad:authored-product")
            .unwrap()
            .len(),
        2
    );
    let entitlements = catalogue
        .related("entitlement", "gumroad:authored-product")
        .unwrap();
    assert_eq!(entitlements.len(), 2);
    assert!(entitlements
        .iter()
        .all(|row| row["manifestComplete"] == false));
    let (groups, blocked, unbound) =
        grouped_journals(&catalogue, &job, account, &checkpoint, &BTreeMap::new()).unwrap();
    assert!(blocked.is_empty() && !unbound);
    for (product, records) in groups {
        commit_gumroad_product(&catalogue, &job, &product, &records, &mut checkpoint).unwrap();
    }
    assert!(catalogue
        .related("entitlement", "gumroad:authored-product")
        .unwrap()
        .iter()
        .all(|grant| grant["manifestComplete"] == true));
    for journal in catalogue.list("checkpoint").unwrap() {
        assert!(!journal.to_string().contains("AUTHORED_CAPABILITY"));
    }
    catalogue
        .put(
            "job",
            "authored-content-job",
            json!({"id":"authored-content-job","state":"cancelled"}),
        )
        .unwrap();
    let third = content(
        "authored-product",
        "third-purchase",
        "third-file",
        "AUTHORED_CAPABILITY",
    );
    assert_eq!(
        service
            .consume_complete_observation(
                &job,
                account,
                &card(&third),
                &third,
                None,
                None,
                false,
                &mut checkpoint
            )
            .unwrap_err()
            .to_string(),
        "CANCELLED"
    );
    assert_eq!(
        catalogue
            .related("remote_file", "gumroad:authored-product")
            .unwrap()
            .len(),
        2
    );
    drop(stream);
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
use super::*;
#[tokio::test]
async fn concurrent_grants_share_one_public_observation_including_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for fail in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let reads = Mutex::new(BTreeMap::new());
        let mut grants = futures_util::stream::iter(0..2)
            .map(|_| {
                let calls = calls.clone();
                let read = shared_public_read(&reads, "authored-product", || {
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        if fail {
                            Err("GUMROAD_PUBLIC_METADATA_UNAVAILABLE".into())
                        } else {
                            Ok(None)
                        }
                    }
                    .boxed()
                    .shared()
                })
                .unwrap();
                read
            })
            .buffered(2);
        let mut results = Vec::new();
        while let Some(result) = grants.next().await {
            results.push(result);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.is_err() == fail));
    }
}
#[test]
fn skipping_unbound_option_quote_preserves_files_paid_evidence_and_source_fields() {
    let path =
        std::env::temp_dir().join(format!("nna-gumroad-option-skip-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "authored-account";
    let product = "gumroad:authored-product";
    let acquisition_id = format!(
        "gumroad:{account}:{}",
        fingerprint(&json!("authored-purchase"))
    );
    let prior = json!({"id":product,"store":"Gumroad","price":12.5,"priceCurrency":"USD","priceBasis":"gumroadPublicPriceCents","sourcePriceCents":1250,"isPayWhatYouWant":false,"sourceDescription":"Preserved source description"});
    catalogue.put("product", product, prior).unwrap();
    catalogue.put("acquisition",&acquisition_id,json!({"id":acquisition_id,"accountId":account,"productId":product,"paidPrice":7.5,"paidCurrency":"USD","paidPriceBasis":"productLineTotal"})).unwrap();
    catalogue
        .put(
            "overlay",
            product,
            json!({"id":product,"note":"Manual note","title":"Manual title"}),
        )
        .unwrap();
    let card = LibraryPurchase {
        purchase_id: "authored-purchase".into(),
        title: Some("Authored product".into()),
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: Some("Owned option".into()),
        archived: false,
        content_route: None,
    };
    let mut observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "AUTHORED_CAPABILITY",
    );
    assert!(needs_public_quote(&observation));
    observation.variant_id = Some("authored-option".into());
    observation.public_product_url =
        Some(Url::parse("https://authored.gumroad.com/l/product").unwrap());
    assert!(!needs_public_quote(&observation));
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &card, &observation).unwrap();
    project_public(&mut batch, &observation, None).unwrap();
    batch.commit().unwrap();
    let stored = catalogue.get("product", product).unwrap().unwrap();
    assert!(stored["price"].is_null());
    assert_eq!(stored["sourcePriceCents"], 1250);
    assert_eq!(stored["sourceDescription"], "Preserved source description");
    assert_eq!(catalogue.related("remote_file", product).unwrap().len(), 1);
    assert!(catalogue
        .related("entitlement", product)
        .unwrap()
        .iter()
        .any(|row| row["manifestComplete"] == true));
    assert_eq!(
        catalogue
            .get("acquisition", &acquisition_id)
            .unwrap()
            .unwrap()["variantId"],
        "authored-option"
    );
    assert_eq!(
        catalogue
            .get("acquisition", &acquisition_id)
            .unwrap()
            .unwrap()["paidPrice"],
        7.5
    );
    assert_eq!(
        catalogue.get("overlay", product).unwrap().unwrap()["note"],
        "Manual note"
    );
}

#[test]
fn public_body_enriches_owned_option_without_promoting_listing_price_or_unknown_body() {
    let path =
        std::env::temp_dir().join(format!("nna-gumroad-public-body-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "authored-account";
    let product = "gumroad:authored-product";
    let acquisition_id = format!(
        "gumroad:{account}:{}",
        fingerprint(&json!("authored-purchase"))
    );
    catalogue
        .put(
            "product",
            product,
            json!({
                "id":product, "store":"Gumroad", "provider":"gumroad",
                "sourceDescription":"Previously observed VRChat body", "sourcePriceCents":1250,
                "price":12.5, "priceCurrency":"USD", "priceBasis":"gumroadPublicPriceCents",
                "isPayWhatYouWant":false, "isRecurringBilling":false
            }),
        )
        .unwrap();
    catalogue
        .put(
            "acquisition",
            &acquisition_id,
            json!({
                "id":acquisition_id, "accountId":account, "productId":product,
                "paidPrice":7.5, "paidCurrency":"USD", "paidPriceBasis":"productLineTotal"
            }),
        )
        .unwrap();
    let card = LibraryPurchase {
        purchase_id: "authored-purchase".into(),
        title: Some("Authored hair".into()),
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: Some("Owned option".into()),
        archived: false,
        content_route: None,
    };
    let mut observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "AUTHORED_CAPABILITY",
    );
    observation.variant_id = Some("authored-option".into());
    observation.public_product_url =
        Some(Url::parse("https://authored.gumroad.com/l/product").unwrap());
    assert!(!needs_public_quote(&observation));
    for (body, expected) in [
        (
            Some(json!("<p>Hair &amp; accessories for VRChat</p>")),
            "Hair & accessories for VRChat",
        ),
        (None, "Hair & accessories for VRChat"),
        (Some(Value::Null), "Hair & accessories for VRChat"),
        (Some(json!("")), ""),
    ] {
        let mut value = json!({"api_version":1,"id":"authored-product",
                "price_cents":9999,"currency_code":"usd","price_formatted":"$99.99",
                "is_pay_what_you_want":true,"is_recurring_billing":true});
        if let Some(body) = body {
            value["description"] = body;
        }
        let public = gumroad::parse_public_product(&value, Some("authored-product")).unwrap();
        let mut batch = RecordBatch::new(&catalogue);
        project(&mut batch, account, &card, &observation).unwrap();
        project_public(&mut batch, &observation, Some(&public)).unwrap();
        batch.commit().unwrap();
        let stored = catalogue.get("product", product).unwrap().unwrap();
        assert_eq!(stored["sourceDescription"], expected);
        for field in ["price", "priceCurrency", "priceBasis", "priceFormatted"] {
            assert!(stored[field].is_null(), "{field}");
        }
        assert_eq!(stored["sourcePriceCents"], 1250);
        assert_eq!(stored["isPayWhatYouWant"], false);
        assert_eq!(stored["isRecurringBilling"], false);
        assert_eq!(stored["priceRefreshState"], "unavailable");
        assert_eq!(
            catalogue
                .get("acquisition", &acquisition_id)
                .unwrap()
                .unwrap()["paidPrice"],
            7.5
        );
        assert_eq!(catalogue.related("remote_file", product).unwrap().len(), 1);
    }
}

#[test]
fn terminal_auth_failure_expires_account_without_erasing_binding_or_ownership() {
    let path = std::env::temp_dir().join(format!("nna-gumroad-terminal-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let account_id = "11111111-1111-4111-8111-111111111111";
    let baseline = json!({"id":account_id,"store":"Gumroad","state":"connected","status":"connected","identityVerified":true,"identityHash":"authored-buyer-hash","authenticatedAt":"2026-10-08T01:02:03Z","disconnected":false,"lastSync":"2026-10-08T01:02:03Z","lastSyncAt":"2026-10-08T01:02:03Z"});
    let owned = json!({"id":"authored-acquisition","accountId":account_id,"productId":"gumroad:authored-product","purchaseId":"authored-purchase"});
    catalogue
        .put("acquisition", "authored-acquisition", owned.clone())
        .unwrap();
    for kind in ["gumroadSync", "gumroadUpdate"] {
        for error in [
            Some("AUTH_REQUIRED"),
            Some("NETWORK_UNAVAILABLE"),
            Some("CANCELLED"),
            None,
        ] {
            catalogue
                .put("account", account_id, baseline.clone())
                .unwrap();
            let job = json!({"id":"authored-terminal-job","type":kind,"store":"Gumroad","accountId":account_id,"state":"running","progress":{"pages":0,"stage":"collecting"}});
            catalogue
                .put("job", "authored-terminal-job", job.clone())
                .unwrap();
            if error == Some("CANCELLED") {
                let mut cancellation = job.clone();
                cancellation["state"] = json!("cancelled");
                catalogue
                    .put("job", "authored-terminal-job", cancellation)
                    .unwrap();
            }
            catalogue
                .command(
                    "preferences",
                    json!({"patch":{"avatarAutomaticEnabled":{"booth:3390957":true}}}),
                )
                .unwrap();
            let other_pending = kind == "gumroadSync" && error == Some("AUTH_REQUIRED");
            if other_pending {
                catalogue
                    .put(
                        "job",
                        "other-provider-sync",
                        json!({"id":"other-provider-sync","type":"jinxxySync","state":"queued"}),
                    )
                    .unwrap();
            }
            let reported_error = if error == Some("CANCELLED") {
                None
            } else {
                error
            };
            service.finish_job(&job, reported_error).unwrap();
            if other_pending {
                assert_eq!(
                    catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"]
                        ["booth:3390957"],
                    true,
                    "A pending other-provider sync defers the terminal hook"
                );
                catalogue
                    .put(
                        "job",
                        "other-provider-sync",
                        json!({"id":"other-provider-sync","type":"jinxxySync","state":"cancelled"}),
                    )
                    .unwrap();
                service.finish_job(&job, error).unwrap();
            }
            assert_eq!(
                catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
                json!({}),
                "The actual terminal hook recomputes automatic defaults"
            );
            let account = catalogue.get("account", account_id).unwrap().unwrap();
            let terminal = catalogue
                .get("job", "authored-terminal-job")
                .unwrap()
                .unwrap();
            let expected_state = if error == Some("AUTH_REQUIRED") {
                "expired"
            } else {
                "connected"
            };
            assert_eq!(account["state"], expected_state);
            assert_eq!(account["status"], expected_state);
            for field in [
                "identityVerified",
                "identityHash",
                "authenticatedAt",
                "disconnected",
            ] {
                assert_eq!(account[field], baseline[field]);
            }
            if error.is_some() || kind == "gumroadUpdate" {
                assert_eq!(account["lastSync"], baseline["lastSync"]);
                assert_eq!(account["lastSyncAt"], baseline["lastSyncAt"]);
            } else {
                assert_eq!(account["lastSyncAt"], terminal["completedAt"]);
            }
            assert_eq!(
                terminal["state"],
                if error == Some("CANCELLED") {
                    "cancelled"
                } else if error.is_some() {
                    "needsAction"
                } else {
                    "completed"
                }
            );
            assert_eq!(terminal["errorCode"], json!(error));
            assert_eq!(
                catalogue
                    .get("acquisition", "authored-acquisition")
                    .unwrap(),
                Some(owned.clone())
            );
        }
    }
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn download_cache_requires_current_identity_entitlement_revision_and_short_lifetimes() {
    let clock = Instant::now();
    let account = json!({"identityVerified":true,"identityHash":"authored-buyer-hash","status":"connected","state":"connected","disconnected":false});
    let mut session = DownloadSession {
        client: reqwest::Client::new(),
        jar: Arc::new(reqwest::cookie::Jar::default()),
        identity: "authored-buyer-hash".into(),
        verified_at: clock,
        manifests: BTreeMap::new(),
        purchases: BTreeMap::new(),
    };
    assert!(session.valid(&account, clock + Duration::from_secs(119)));
    assert!(!session.valid(&account, clock + Duration::from_secs(120)));
    let mut changed = account.clone();
    changed["identityHash"] = json!("another-authored-buyer");
    assert!(!session.valid(&changed, clock));
    let indexed_card = LibraryPurchase {
        purchase_id: "authored-indexed-purchase".into(),
        title: None,
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: None,
        archived: false,
        content_route: None,
    };
    session.index_purchases(std::slice::from_ref(&indexed_card));
    let sought = BTreeSet::from([indexed_card.purchase_id.as_str()]);
    assert_eq!(session.indexed_purchases(&account, &sought, clock).len(), 1);
    assert!(session
        .indexed_purchases(&account, &BTreeSet::from(["unseen-purchase"]), clock)
        .is_empty());
    assert!(session
        .indexed_purchases(&changed, &sought, clock)
        .is_empty());
    assert!(session
        .indexed_purchases(&account, &sought, clock + Duration::from_secs(120))
        .is_empty());
    for index in 0..10000 {
        let mut card = indexed_card.clone();
        card.purchase_id = format!("authored-index-{index}");
        session.index_purchases(&[card]);
    }
    assert_eq!(session.purchases.len(), 10000);
    assert!(!session.purchases.contains_key("authored-index-9999"));
    assert!(session.snapshot().purchases.is_empty());
    changed = account.clone();
    changed["status"] = json!("disconnected");
    assert!(!session.valid(&changed, clock));
    let observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "authored-capability",
    );
    let offered = &observation.files[0];
    let id = remote_file_id("authored-account", "authored-product", offered);
    let file = json!({"id":id,"accountId":"authored-account","productId":"gumroad:authored-product","sourceProductId":offered.source_product_id,"sourcePurchaseId":offered.source_purchase_id,"sourceFileId":offered.file_id,"purchaseIds":["authored-purchase"],"sourceFingerprint":source_fingerprint(offered),"revision":"authored-revision"});
    let manifest = DownloadManifest {
        purchase_ids: BTreeSet::from([observation.purchase_id]),
        source_bundle_id: None,
        observed_at: clock,
        files: observation.files,
        revisions: BTreeMap::from([(id, "authored-revision".into())]),
    };
    assert!(manifest
        .offered(
            "authored-account",
            "authored-product",
            &file,
            clock + Duration::from_secs(59)
        )
        .is_some());
    assert!(manifest
        .offered(
            "authored-account",
            "authored-product",
            &file,
            clock + Duration::from_secs(60)
        )
        .is_none());
    for (key, value) in [
        ("revision", json!("changed")),
        ("sourceProductId", json!("other-product")),
        ("sourcePurchaseId", json!("other-purchase")),
        ("sourceFileId", json!("other-file")),
        ("purchaseIds", json!(["other-purchase"])),
    ] {
        let mut changed = file.clone();
        changed[key] = value;
        assert!(manifest
            .offered("authored-account", "authored-product", &changed, clock)
            .is_none());
    }
    session
        .manifests
        .insert("gumroad:authored-product".into(), manifest);
    assert!(session.snapshot().manifests.is_empty());
    assert_eq!(session.manifests.len(), 1);
}

#[test]
fn optional_public_failure_keeps_complete_bound_product_publishable() {
    let path = std::env::temp_dir().join(format!("nna-gumroad-public-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let product = "gumroad:authored-product";
    let account = "authored-account";
    let job = json!({"id":"authored-job","state":"running","accountId":account,"store":"Gumroad","type":"gumroadSync"});
    catalogue.put("job", "authored-job", job.clone()).unwrap();
    catalogue.put("product", product, json!({"id":product,"store":"Gumroad","price":12.5,"priceCurrency":"USD","priceBasis":"gumroadPublicPriceCents","sourceTags":["Authored source tag","Authored rejected tag"]})).unwrap();
    catalogue
        .put("account", account, json!({"id":account,"store":"Gumroad"}))
        .unwrap();
    let overlay = json!({"id":product,"title":"Human Gumroad title","note":"Human note","category":"Hair","tags":["Authored manual tag"],"rejectedTags":["Authored rejected tag"]});
    catalogue.put("overlay", product, overlay.clone()).unwrap();
    let card = LibraryPurchase {
        purchase_id: "authored-purchase".into(),
        title: Some("Authored complete product".into()),
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: None,
        archived: false,
        content_route: None,
    };
    let observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "AUTHORED_CAPABILITY",
    );
    assert!(observation.inventory_complete);
    // An already proven historical payment is independent of the fresh
    // public quote. The current Gumroad parser does not infer a new payment.
    let acquisition_id = format!(
        "gumroad:{account}:{}",
        fingerprint(&json!(observation.purchase_id))
    );
    catalogue.put("acquisition", &acquisition_id, json!({"id":acquisition_id,"productId":product,"accountId":account,"store":"Gumroad","provider":"gumroad","source":"library","purchaseId":observation.purchase_id,"paidPrice":7.5,"paidCurrency":"USD","paidPriceBasis":"productLineTotal"})).unwrap();
    let mut prepared = RecordBatch::new(&catalogue);
    project(&mut prepared, account, &card, &observation).unwrap();
    project_public(&mut prepared, &observation, None).unwrap();
    let records = prepared.staged_records();
    catalogue.put("checkpoint", "authored-journal", json!({"id":"authored-journal","accountId":account,"runId":job["id"],"store":"Gumroad","observations":[{"productId":product,"complete":observation.inventory_complete,"errorCode":"GUMROAD_PUBLIC_SCHEMA_UNSUPPORTED"}],"records":records})).unwrap();
    let mut checkpoint = json!({"id":"authored-checkpoint","journalIds":["authored-journal"],"publishedProducts":[]});
    let (groups, blocked, unbound) =
        grouped_journals(&catalogue, &job, account, &checkpoint, &BTreeMap::new()).unwrap();
    assert!(blocked.is_empty() && !unbound);
    commit_gumroad_product(&catalogue, &job, product, &groups[product], &mut checkpoint).unwrap();
    let published = catalogue.get("product", product).unwrap().unwrap();
    assert_eq!(published["priceRefreshState"], "unavailable");
    assert_eq!(published["price"], 12.5);
    assert_eq!(published["priceCurrency"], "USD");
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 1);
    assert_eq!(
        catalogue.list("entitlement").unwrap()[0]["manifestComplete"],
        true
    );
    let assert_retained = |catalogue: &Catalogue, price: f64| {
        let published = catalogue.get("product", product).unwrap().unwrap();
        assert_eq!(published["price"], price);
        assert_eq!(published["priceCurrency"], "USD");
        assert_eq!(
            published["sourceTags"],
            json!(["Authored source tag", "Authored rejected tag"])
        );
        assert_eq!(catalogue.get("overlay", product).unwrap().unwrap(), overlay);
        let acquisition = catalogue
            .get("acquisition", &acquisition_id)
            .unwrap()
            .unwrap();
        assert_eq!(acquisition["paidPrice"], 7.5);
        assert_eq!(acquisition["paidCurrency"], "USD");
        assert_eq!(acquisition["paidPriceBasis"], "productLineTotal");
        assert_eq!(acquisition["accountId"], account);
        assert_eq!(acquisition["purchaseId"], "authored-purchase");
        assert_eq!(acquisition["acquiredAt"], "2026-10-08T01:02:03+00:00");
        let details = catalogue
            .command("product_details", json!({"id":product}))
            .unwrap();
        assert_eq!(details["price"], price);
        assert_eq!(details["paidPrice"], 7.5);
        assert_eq!(details["accountIds"], json!([account]));
        assert_eq!(details["title"], "Human Gumroad title");
        assert_eq!(details["note"], "Human note");
        assert_eq!(details["category"], "Hair");
        assert!(details["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "Authored manual tag"));
        assert!(!details["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "Authored rejected tag"));
        let grants = catalogue.related("entitlement", product).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0]["accountId"], account);
        assert_eq!(grants[0]["fileIds"].as_array().unwrap().len(), 1);
        assert_eq!(grants[0]["manifestComplete"], true);
    };
    assert_retained(&catalogue, 12.5);
    assert!(!published.to_string().contains("AUTHORED_CAPABILITY"));
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let mut partial = observation.clone();
    partial.inventory_complete = false;
    partial.files.clear();
    assert!(service
        .consume_complete_observation(
            &job,
            account,
            &card,
            &partial,
            None,
            None,
            true,
            &mut checkpoint
        )
        .unwrap()
        .is_empty());
    assert_eq!(
        catalogue.get("product", product).unwrap().unwrap(),
        published
    );
    assert_retained(&catalogue, 12.5);
    drop(service);
    drop(catalogue);
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    assert_retained(&catalogue, 12.5);
    let mut exact_variant = observation.clone();
    exact_variant.variant_id = Some("authored-owned-variant".into());
    let listing = gumroad::parse_public_product(&json!({"api_version":1,"id":"authored-product","price_cents":2000,"currency_code":"usd","price_formatted":"$20","is_pay_what_you_want":false,"is_recurring_billing":false}),Some("authored-product")).unwrap();
    let mut prepared = RecordBatch::new(&catalogue);
    project(&mut prepared, account, &card, &exact_variant).unwrap();
    project_public(&mut prepared, &exact_variant, Some(&listing)).unwrap();
    let exact_variant_records = prepared.staged_records();
    commit_gumroad_product_mode(
        &catalogue,
        &job,
        product,
        &exact_variant_records,
        &mut checkpoint,
        false,
    )
    .unwrap();
    let exact_price = catalogue.get("product", product).unwrap().unwrap();
    assert!(exact_price["price"].is_null());
    assert!(exact_price["priceCurrency"].is_null());
    assert!(exact_price["priceBasis"].is_null());
    assert_eq!(
        catalogue
            .get("acquisition", &acquisition_id)
            .unwrap()
            .unwrap()["paidPrice"],
        7.5
    );
    let exact_details = catalogue
        .command("product_details", json!({"id":product}))
        .unwrap();
    assert!(exact_details["price"].is_null());
    assert_eq!(exact_details["paidPrice"], 7.5);
    assert_eq!(exact_details["accountIds"], json!([account]));
    assert_eq!(exact_details["title"], "Human Gumroad title");
    assert_eq!(exact_details["note"], "Human note");
    assert_eq!(exact_details["category"], "Hair");
    assert!(!exact_details["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tag| tag == "Authored rejected tag"));
    assert_eq!(
        exact_details["rejectedTags"],
        json!(["Authored rejected tag"])
    );
    let mut later = observation;
    later.acquired_at = None;
    let public = gumroad::parse_public_product(&json!({"api_version":1,"id":"authored-product","price_cents":2000,"currency_code":"usd","price_formatted":"$20","is_pay_what_you_want":false,"is_recurring_billing":false}),Some("authored-product")).unwrap();
    let mut prepared = RecordBatch::new(&catalogue);
    project(&mut prepared, account, &card, &later).unwrap();
    project_public(&mut prepared, &later, Some(&public)).unwrap();
    let later_records = prepared.staged_records();
    commit_gumroad_product_mode(
        &catalogue,
        &job,
        product,
        &later_records,
        &mut checkpoint,
        false,
    )
    .unwrap();
    assert_retained(&catalogue, 20.0);
    commit_gumroad_product(&catalogue, &job, product, &later_records, &mut checkpoint).unwrap();
    assert_retained(&catalogue, 20.0);
    drop(prepared);
    drop(service);
    drop(catalogue);
    let reopened = Catalogue::open(&path).unwrap();
    assert_retained(&reopened, 20.0);
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn staged_product_groups_all_purchases_and_cancel_preserves_previous_snapshot() {
    let path = std::env::temp_dir().join(format!("nna-gumroad-staged-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "authored-account";
    let job = json!({"id":"authored-job","state":"running","accountId":account,"store":"Gumroad","type":"gumroadSync"});
    catalogue.put("job", "authored-job", job.clone()).unwrap();
    let prior = json!({"id":"gumroad:authored-product","store":"Gumroad","title":"Previous published","price":5});
    catalogue
        .put("product", "gumroad:authored-product", prior.clone())
        .unwrap();
    catalogue
        .put(
            "overlay",
            "gumroad:authored-product",
            json!({"id":"gumroad:authored-product","note":"Manual note"}),
        )
        .unwrap();
    let mut checkpoint = json!({"id":"authored-checkpoint","journalIds":[],"publishedProducts":[]});
    let public = gumroad::parse_public_product(&json!({"api_version":1,"id":"authored-product","price_cents":19900,"currency_code":"usd","price_formatted":"$199","is_pay_what_you_want":false,"is_recurring_billing":false}),Some("authored-product")).unwrap();
    for (purchase, file) in [
        ("authored-purchase-1", "authored-file-1"),
        ("authored-purchase-2", "authored-file-2"),
    ] {
        let card = LibraryPurchase {
            purchase_id: purchase.into(),
            title: Some("Authored product".into()),
            author: None,
            thumbnail_url: None,
            native_type: Some("digital".into()),
            variant_name: None,
            archived: false,
            content_route: None,
        };
        let mut observation = content("authored-product", purchase, file, "SECRET_CAPABILITY");
        if purchase == "authored-purchase-1" {
            observation.variant_id = Some("authored-variant".into());
        }
        let mut prepared = RecordBatch::new(&catalogue);
        project(&mut prepared, account, &card, &observation).unwrap();
        project_public(&mut prepared, &observation, Some(&public)).unwrap();
        let records = prepared.staged_records();
        let journal = json!({"id":purchase,"accountId":account,"runId":job["id"],"store":"Gumroad","observations":[{"productId":"gumroad:authored-product","complete":true}],"records":records});
        assert!(!journal.to_string().contains("SECRET_CAPABILITY"));
        catalogue.put("checkpoint", purchase, journal).unwrap();
        append_journal(&mut checkpoint, purchase).unwrap();
    }
    assert_eq!(
        catalogue
            .get("product", "gumroad:authored-product")
            .unwrap(),
        Some(prior.clone())
    );
    assert!(catalogue.list("acquisition").unwrap().is_empty());
    let (groups, blocked, unbound) =
        grouped_journals(&catalogue, &job, account, &checkpoint, &BTreeMap::new()).unwrap();
    assert!(blocked.is_empty() && !unbound);
    assert_eq!(groups.len(), 1);
    let records = &groups["gumroad:authored-product"];
    let mut cancelled = job.clone();
    cancelled["state"] = json!("cancelled");
    catalogue.put("job", "authored-job", cancelled).unwrap();
    assert_eq!(
        commit_gumroad_product(
            &catalogue,
            &job,
            "gumroad:authored-product",
            records,
            &mut checkpoint
        )
        .unwrap_err()
        .to_string(),
        "CANCELLED"
    );
    assert_eq!(
        catalogue
            .get("product", "gumroad:authored-product")
            .unwrap(),
        Some(prior)
    );
    assert!(catalogue.list("acquisition").unwrap().is_empty());
    catalogue.put("job", "authored-job", job.clone()).unwrap();
    commit_gumroad_product(
        &catalogue,
        &job,
        "gumroad:authored-product",
        records,
        &mut checkpoint,
    )
    .unwrap();
    let published = catalogue
        .get("product", "gumroad:authored-product")
        .unwrap()
        .unwrap();
    assert!(published["price"].is_null() && published["priceFormatted"].is_null());
    assert_eq!(published["sourcePriceCents"], 19900);
    assert_eq!(catalogue.list("acquisition").unwrap().len(), 2);
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 2);
    let entitlements = catalogue.list("entitlement").unwrap();
    assert_eq!(entitlements.len(), 2);
    assert!(entitlements
        .iter()
        .all(|value| value["manifestComplete"] == true));
    assert_eq!(
        catalogue
            .get("overlay", "gumroad:authored-product")
            .unwrap()
            .unwrap()["note"],
        "Manual note"
    );
    assert_eq!(
        checkpoint["publishedProducts"],
        json!(["gumroad:authored-product"])
    );
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

fn content(product: &str, purchase: &str, file: &str, capability: &str) -> ContentObservation {
    gumroad::parse_content_props(&json!({"purchase":{"id":purchase,"product_id":product,"created_at":"2026-10-08T01:02:03Z"},"content":{"content_items":[{"type":"file","id":file,"file_name":"fixture.unitypackage","file_size":42,"pdf_stamp_enabled":false,"download_url":format!("/r/{capability}/product_files?product_file_ids[]={file}")}]}}),
            &Url::parse("https://gumroad.com/d/authored-capability").unwrap(), purchase, Some(product)).unwrap()
}

#[tokio::test]
async fn verification_bundle_cap_does_not_claim_complete_membership() {
    for pages in [2, 6] {
        let fetched = Arc::new(Mutex::new(Vec::new()));
        let members = enumerate_bundle_members("authored-buyer", Some(5), |page, archived| {
            let fetched = fetched.clone();
            async move {
                fetched.lock().unwrap().push((page, archived));
                Ok(LibraryPage {
                    buyer_identity: "authored-buyer".into(),
                    pages: if archived { 1 } else { pages },
                    count: if archived { 1 } else { pages },
                    purchases: vec![LibraryPurchase {
                        purchase_id: format!("authored-{archived}-{page}"),
                        title: None,
                        author: None,
                        thumbnail_url: None,
                        native_type: Some("digital".into()),
                        variant_name: None,
                        archived,
                        content_route: None,
                    }],
                    bundles: vec![],
                })
            }
            .boxed()
        })
        .await
        .unwrap();
        assert_eq!(members.pages, pages.min(5) + 1);
        assert!(fetched.lock().unwrap().iter().all(|(page, _)| *page <= 5));
        assert_eq!(members.complete, pages <= 5);
        let bundle = gumroad::LibraryBundle {
            product_id: "authored-parent".into(),
            label: None,
        };
        let reuse = members
            .purchases
            .iter()
            .map(|row| {
                (
                    row.purchase_id.clone(),
                    Some(content(
                        "authored-child",
                        &row.purchase_id,
                        &row.purchase_id,
                        "AUTHORED_CAPABILITY",
                    )),
                )
            })
            .collect();
        let (_, parent) = assemble_observed_bundle(&bundle, &members, &reuse);
        assert_eq!(parent.files.len(), members.purchases.len());
        assert_eq!(parent.inventory_complete, pages <= 5);
        assert_eq!(parent.unsupported_items, usize::from(pages > 5));
    }
}

#[tokio::test]
async fn protected_bundle_lists_precede_ordinary_publication_then_parent_content() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let bundle = gumroad::LibraryBundle {
        product_id: "authored-parent".into(),
        label: Some("Authored parent".into()),
    };
    let members = enumerate_bundle_members("authored-buyer", None, |page, archived| {
            let events = events.clone();
            async move {
                events.lock().unwrap().push(format!("members:{archived}:{page}"));
                let purchase = if archived { "archived-member" } else if page == 1 { "first-member" } else { "second-member" };
                let pages = if archived { 1 } else { 2 };
                let props = json!({"buyerIdentity":"authored-buyer","bundles":[{"id":"authored-parent","label":"Authored parent"}],"results":[{"product":{"name":"Authored child","native_type":"digital"},"purchase":{"id":purchase,"is_archived":archived,"download_url":"/d/AUTHORED_CAPABILITY"}}],"pagination":{"page":page,"pages":pages,"count":pages,"from":page,"to":page},"archived_count":1,"unarchived_count":2,"search":{"show_archived_only":archived,"query":"","creators":[],"bundles":["authored-parent"],"sort":"purchase_date"},"bundle_downloads":[{"id":"authored-parent","download_url":null,"zip_unavailable_reason":null}]});
                gumroad::parse_library_scope(&props, &Url::parse("https://gumroad.com/library?bundles=authored-parent").unwrap(), page, archived, Some("authored-parent"))
            }.boxed()
        }).await.unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["members:false:1", "members:false:2", "members:true:1"]
    );
    assert_eq!(members.pages, 3);
    let bundled: BTreeSet<_> = members
        .purchases
        .iter()
        .map(|row| row.purchase_id.clone())
        .collect();
    assert_eq!(bundled.len(), 3);
    assert!(members
        .purchases
        .iter()
        .all(|row| !parentless_purchase(row, &bundled)));
    let mut ordinary = members.purchases[0].clone();
    ordinary.purchase_id = "separate-purchase".into();
    assert!(parentless_purchase(&ordinary, &bundled));
    let path = std::env::temp_dir().join(format!("gumroad-bundle-order-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let account = "authored-account";
    let job = json!({"id":"authored-order-job","state":"running","accountId":account,"store":"Gumroad","type":"gumroadSync"});
    catalogue
        .put("job", "authored-order-job", job.clone())
        .unwrap();
    let mut checkpoint = json!({"id":"authored-order-checkpoint","journalIds":[],"publishedProducts":[],"bundlePurchaseIds":bundled});
    let observation = content(
        "authored-child",
        "separate-purchase",
        "ordinary-file",
        "AUTHORED_OTHER_CAPABILITY",
    );
    service
        .consume_complete_observation(
            &job,
            account,
            &ordinary,
            &observation,
            None,
            None,
            false,
            &mut checkpoint,
        )
        .unwrap();
    events.lock().unwrap().push("ordinary-published".into());
    assert!(catalogue
        .get("product", "gumroad:authored-child")
        .unwrap()
        .is_some());
    assert!(catalogue
        .get("product", "gumroad:authored-parent")
        .unwrap()
        .is_none());
    let mut reuse = BTreeMap::new();
    assert!(
        !assemble_observed_bundle(&bundle, &members, &reuse)
            .1
            .inventory_complete
    );
    for row in &members.purchases {
        events
            .lock()
            .unwrap()
            .push(format!("child-content:{}", row.purchase_id));
        reuse.insert(
            row.purchase_id.clone(),
            Some(content(
                "authored-child",
                &row.purchase_id,
                &format!("file-{}", row.purchase_id),
                "AUTHORED_CAPABILITY",
            )),
        );
    }
    let (card, parent) = assemble_observed_bundle(&bundle, &members, &reuse);
    assert!(parent.inventory_complete);
    assert_eq!(parent.member_purchase_ids, bundled);
    assert_eq!(parent.files.len(), 3);
    let journal_id = "authored-parent-journal";
    let mut prepared = RecordBatch::new(&catalogue);
    project(&mut prepared, account, &card, &parent).unwrap();
    catalogue.put("checkpoint", journal_id, json!({"id":journal_id,"store":"Gumroad","accountId":account,"runId":job["id"],"observations":[{"productId":"gumroad:authored-parent","complete":true}],"records":prepared.staged_records()})).unwrap();
    append_journal(&mut checkpoint, journal_id).unwrap();
    service
        .publish_complete_journal_records(&job, &mut checkpoint, journal_id)
        .unwrap();
    assert!(catalogue
        .get("product", "gumroad:authored-parent")
        .unwrap()
        .is_some());
    assert_eq!(events.lock().unwrap()[3], "ordinary-published");
    assert!(events.lock().unwrap()[4..]
        .iter()
        .all(|event| event.starts_with("child-content:")));
    let grants = catalogue
        .related("entitlement", "gumroad:authored-parent")
        .unwrap();
    assert_eq!(grants[0]["manifestComplete"], false);
    assert!(catalogue
        .related("acquisition", "gumroad:authored-parent")
        .unwrap()[0]["paidPrice"]
        .is_null());
    let (groups, blocked, unbound) =
        grouped_journals(&catalogue, &job, account, &checkpoint, &BTreeMap::new()).unwrap();
    assert!(blocked.is_empty() && !unbound);
    for (product, records) in groups {
        commit_gumroad_product(&catalogue, &job, &product, &records, &mut checkpoint).unwrap();
    }
    assert_eq!(
        catalogue
            .related("entitlement", "gumroad:authored-parent")
            .unwrap()[0]["manifestComplete"],
        true
    );
    assert!(
        !serde_json::to_string(&catalogue.list("remote_file").unwrap())
            .unwrap()
            .contains("CAPABILITY")
    );
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn hidden_bundle_parent_keeps_real_member_binding_and_separate_child_purchase() {
    let base = Url::parse("https://gumroad.com/library?bundles=authored-parent").unwrap();
    let props = json!({"buyerIdentity":"authored-buyer","bundles":[{"id":"authored-parent","label":"Authored bundle"}],"results":[{"product":{"name":"Authored child","native_type":"digital"},"purchase":{"id":"authored-member-purchase","is_archived":false,"download_url":"/d/AUTHORED_CAPABILITY"}}],"pagination":{"page":1,"pages":1,"count":1,"from":1,"to":1},"archived_count":0,"unarchived_count":2,"search":{"show_archived_only":false,"query":"","creators":[],"bundles":["authored-parent"],"sort":"purchase_date"},"bundle_downloads":[{"id":"authored-parent","download_url":null,"zip_unavailable_reason":null}]});
    assert!(gumroad::parse_library_scope(&props, &base, 1, false, Some("wrong-parent")).is_err());
    assert!(gumroad::parse_library_props(&props, &base, 1, false).is_err());
    let page =
        gumroad::parse_library_scope(&props, &base, 1, false, Some("authored-parent")).unwrap();
    let (card, mut parent) = bundle_parent(&page.bundles[0]);
    let bundled_child = content(
        "authored-child",
        "authored-member-purchase",
        "authored-file",
        "AUTHORED_CAPABILITY",
    );
    parent
        .member_purchase_ids
        .insert(bundled_child.purchase_id.clone());
    merge_member_files(&mut parent, bundled_child.files);
    parent.inventory_complete = true;
    let mut separate_card = page.purchases[0].clone();
    separate_card.purchase_id = "authored-separate-purchase".into();
    assert!(!parentless_purchase(
        &page.purchases[0],
        &parent.member_purchase_ids
    ));
    assert!(parentless_purchase(
        &separate_card,
        &parent.member_purchase_ids
    ));
    let separate_child = content(
        "authored-child",
        "authored-separate-purchase",
        "authored-file",
        "AUTHORED_OTHER_CAPABILITY",
    );
    let path = std::env::temp_dir().join(format!(
        "nna-authored-gumroad-bundle-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &card, &parent).unwrap();
    project(&mut batch, account, &separate_card, &separate_child).unwrap();
    batch.commit().unwrap();
    let acquisitions = catalogue.list("acquisition").unwrap();
    assert_eq!(acquisitions.len(), 2);
    let bundle = acquisitions
        .iter()
        .find(|row| row["sourceBundleId"] == "authored-parent")
        .unwrap();
    assert!(bundle.get("purchaseId").is_none());
    assert!(bundle["acquiredAt"].is_null() && bundle["paidPrice"].is_null());
    assert!(acquisitions
        .iter()
        .any(|row| row["purchaseId"] == "authored-separate-purchase"));
    let files = catalogue.list("remote_file").unwrap();
    assert_eq!(files.len(), 2);
    let bundled_file = files
        .iter()
        .find(|file| file["sourceBundleId"] == "authored-parent")
        .unwrap();
    assert_eq!(bundled_file["productId"], "gumroad:authored-parent");
    assert_eq!(bundled_file["sourceProductId"], "authored-child");
    assert_eq!(bundled_file["sourcePurchaseId"], "authored-member-purchase");
    assert_eq!(
        bundled_file["purchaseIds"],
        json!(["authored-member-purchase"])
    );
    let entitlement = catalogue
        .list("entitlement")
        .unwrap()
        .into_iter()
        .find(|row| row["sourceBundleId"] == "authored-parent")
        .unwrap();
    assert_eq!(entitlement["manifestComplete"], true);
    assert!(entitlement.get("purchaseId").is_none());
    for kind in ["product", "acquisition", "remote_file", "entitlement"] {
        assert!(!serde_json::to_string(&catalogue.list(kind).unwrap())
            .unwrap()
            .contains("CAPABILITY"));
    }
    drop(catalogue);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn bundle_files_keep_member_identity_and_ambiguous_duplicates_are_unavailable() {
    let mut parent = content(
        "authored-parent",
        "authored-parent-purchase",
        "authored-parent-file",
        "parent-capability",
    );
    let child = content(
        "authored-member",
        "authored-member-purchase",
        "authored-shared-file",
        "child-capability",
    );
    merge_member_files(&mut parent, child.files);
    assert_eq!(parent.files.len(), 2);
    assert_eq!(parent.files[1].source_product_id, "authored-member");
    assert_eq!(
        parent.files[1].source_purchase_id,
        "authored-member-purchase"
    );
    let mut same_id_other_product = content(
        "authored-other-member",
        "authored-other-purchase",
        "authored-shared-file",
        "other-capability",
    );
    merge_member_files(
        &mut parent,
        std::mem::take(&mut same_id_other_product.files),
    );
    assert_eq!(parent.files.len(), 3);
    let mut ambiguous = content(
        "authored-member",
        "authored-member-purchase",
        "authored-shared-file",
        "fresh-capability",
    );
    ambiguous.files[0].name = Some("different.zip".into());
    merge_member_files(&mut parent, ambiguous.files);
    assert_eq!(parent.files[1].access, FileAccess::Unavailable);
    assert!(parent.files[1].resolver.is_none());
    assert_eq!(parent.unsupported_items, 1);
    assert!(parent.acquired_at.is_some());
}

#[test]
fn projection_date_and_capability_changes_keep_identity_and_never_persist_routes() {
    let path = std::env::temp_dir().join(format!("nna-authored-gumroad-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    let card = LibraryPurchase {
        purchase_id: "authored-purchase".into(),
        title: Some("Authored product".into()),
        author: None,
        thumbnail_url: None,
        native_type: Some("digital".into()),
        variant_name: None,
        archived: false,
        content_route: None,
    };
    let mut observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "AUTHORED_SECRET_CAPABILITY",
    );
    observation.acquired_at = None;
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &card, &observation).unwrap();
    batch.commit().unwrap();
    let first = catalogue.list("acquisition").unwrap();
    let first_file = catalogue.list("remote_file").unwrap()[0]["id"].clone();
    observation = content(
        "authored-product",
        "authored-purchase",
        "authored-file",
        "AUTHORED_NEW_SECRET_CAPABILITY",
    );
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &card, &observation).unwrap();
    batch.commit().unwrap();
    let acquisitions = catalogue.list("acquisition").unwrap();
    assert_eq!(acquisitions.len(), 1);
    assert_eq!(acquisitions[0]["id"], first[0]["id"]);
    assert_eq!(
        acquisitions[0]["acquiredDateText"],
        "2026-10-08T01:02:03+00:00"
    );
    assert!(acquisitions[0]["paidPrice"].is_null());
    let public = gumroad::parse_public_product(&json!({"api_version":1,"id":"authored-product","price_cents":19900,"currency_code":"usd","price_formatted":"$199","is_pay_what_you_want":false,"is_recurring_billing":false}), Some("authored-product")).unwrap();
    let mut batch = RecordBatch::new(&catalogue);
    project_public(&mut batch, &observation, Some(&public)).unwrap();
    batch.commit().unwrap();
    let product = catalogue
        .get("product", "gumroad:authored-product")
        .unwrap()
        .unwrap();
    assert_eq!(product["price"], 199.0);
    assert_eq!(product["priceCurrency"], "USD");
    assert_eq!(product["priceFormatted"], "$199");
    let mut batch = RecordBatch::new(&catalogue);
    project_public(&mut batch, &observation, None).unwrap();
    batch.commit().unwrap();
    let product = catalogue
        .get("product", "gumroad:authored-product")
        .unwrap()
        .unwrap();
    assert_eq!(product["price"], 199.0);
    assert_eq!(product["priceRefreshState"], "unavailable");
    observation.variant_id = Some("authored-variant".into());
    let mut batch = RecordBatch::new(&catalogue);
    project_public(&mut batch, &observation, None).unwrap();
    batch.commit().unwrap();
    let variant = catalogue
        .get("product", "gumroad:authored-product")
        .unwrap()
        .unwrap();
    assert!(variant["price"].is_null() && variant["priceFormatted"].is_null());
    assert_eq!(variant["sourcePriceCents"], 19900);
    observation.variant_id = None;
    assert!(catalogue.list("acquisition").unwrap()[0]["paidPrice"].is_null());
    let files = catalogue.list("remote_file").unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["id"], first_file);
    let mut reference = observation.files[0].clone();
    reference.file_id = "authored-external-reference".into();
    reference.access = FileAccess::ExternalReference;
    reference.resolver = None;
    observation.files.push(reference.clone());
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &card, &observation).unwrap();
    batch.commit().unwrap();
    let entitlement = catalogue.list("entitlement").unwrap().remove(0);
    assert_eq!(entitlement["manifestComplete"], true);
    assert_eq!(entitlement["fileIds"], json!([first_file]));
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 1);

    let mut external_card = card.clone();
    external_card.purchase_id = "authored-external-purchase".into();
    observation.purchase_id = external_card.purchase_id.clone();
    observation.product_id = "authored-external-product".into();
    reference.source_purchase_id = observation.purchase_id.clone();
    reference.source_product_id = observation.product_id.clone();
    observation.files = vec![reference];
    let mut batch = RecordBatch::new(&catalogue);
    project(&mut batch, account, &external_card, &observation).unwrap();
    batch.commit().unwrap();
    let entitlement = catalogue
        .list("entitlement")
        .unwrap()
        .into_iter()
        .find(|row| row["productId"] == "gumroad:authored-external-product")
        .unwrap();
    assert_eq!(entitlement["manifestComplete"], true);
    assert_eq!(entitlement["fileIds"], json!([]));
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 1);
    assert!(catalogue
        .get("product", "gumroad:authored-external-product")
        .unwrap()
        .is_some());
    assert!(catalogue
        .list("acquisition")
        .unwrap()
        .iter()
        .any(
            |row| row["productId"] == "gumroad:authored-external-product"
                && row["acquiredAt"].is_string()
        ));
    for kind in ["product", "acquisition", "entitlement", "remote_file"] {
        let values = catalogue.list(kind).unwrap();
        let serialized = serde_json::to_string(&values).unwrap();
        assert!(!serialized.contains("SECRET_CAPABILITY"));
        assert!(!serialized.contains("/d/") && !serialized.contains("/r/"));
    }
    drop(catalogue);
    let _ = std::fs::remove_dir_all(path);
}

#[cfg(windows)]
#[test]
fn sign_in_answer_keeps_the_protected_session_and_only_marks_the_account() {
    use tauri::webview::Cookie;
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "nna-authored-gumroad-session-{}",
            uuid::Uuid::new_v4()
        ));
    let library = base.join("library");
    std::fs::create_dir_all(&library).unwrap();
    let catalogue = Catalogue::open_bound(&library, &base.join("device")).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let account_id = uuid::Uuid::new_v4().to_string();
    let account = json!({"id":account_id,"store":"Gumroad","state":"connected","status":"connected","identityVerified":true,"identityHash":"authored-buyer-hash","disconnected":false});
    let profile =
        session::profile_path(&catalogue.device_dir(), StoreProvider::Gumroad, &account_id)
            .unwrap();
    let cookie = Cookie::build(("authored_session", "AUTHORED_COOKIE_VALUE"))
        .domain("gumroad.com")
        .path("/")
        .secure(true)
        .build();
    crate::stores::session_cache::save(&profile, StoreProvider::Gumroad, &account_id, vec![cookie])
        .unwrap();
    for kind in ["gumroadSync", "gumroadUpdate"] {
        catalogue
            .put("account", &account_id, account.clone())
            .unwrap();
        let job = json!({"id":"authored-auth-job","type":kind,"store":"Gumroad","accountId":account_id,"state":"running","progress":{"stage":"collecting"}});
        catalogue
            .put("job", "authored-auth-job", job.clone())
            .unwrap();
        service.finish_job(&job, Some("AUTH_REQUIRED")).unwrap();
        let terminal = catalogue.get("job", "authored-auth-job").unwrap().unwrap();
        assert_eq!(terminal["state"], "needsAction");
        assert_eq!(terminal["errorCode"], "AUTH_REQUIRED");
        // The shared record stays as it was for other devices. This device keeps the
        // snapshot for the next sign-in and shows the account signed out until then.
        let stored = catalogue.get("account", &account_id).unwrap().unwrap();
        for field in [
            "state",
            "status",
            "identityVerified",
            "identityHash",
            "disconnected",
        ] {
            assert_eq!(stored[field], account[field]);
        }
        assert!(
            !crate::stores::session_cache::load(&profile, StoreProvider::Gumroad, &account_id)
                .unwrap()
                .is_empty()
        );
        assert!(!crate::stores::has_local_session(&catalogue, &stored));
        // A sign-in saves the session again and this device is ready.
        let cookies =
            crate::stores::session_cache::load(&profile, StoreProvider::Gumroad, &account_id)
                .unwrap();
        crate::stores::session_cache::save(&profile, StoreProvider::Gumroad, &account_id, cookies)
            .unwrap();
        assert!(crate::stores::has_local_session(&catalogue, &stored));
    }
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn one_page_sign_in_answer_fails_only_its_asset_while_outages_stop_the_job() {
    use reqwest::{ResponseBuilderExt, StatusCode};
    let page = |component: &str| {
        let payload = serde_json::to_string(&json!({"component":component,"url":"/d/authored","props":{"purchase":{"id":"authored-purchase"}}}))
                .unwrap()
                .replace('&', "&amp;")
                .replace('"', "&quot;");
        (
            StatusCode::OK,
            format!("<div data-page=\"{payload}\"></div>"),
        )
    };
    // A content page's 401 or login route, rendered login page or 403 denies that asset.
    for response in [
        Err(anyhow::anyhow!("AUTH_REQUIRED")),
        Ok(page("Logins/New")),
        Err(anyhow::anyhow!("PERMISSION_DENIED")),
    ] {
        let code = safe_code(&content_page(response).unwrap_err());
        assert_eq!(code, "PERMISSION_DENIED");
        assert!(!job_level(&code));
    }
    for response in [
        Ok((StatusCode::NOT_FOUND, String::new())),
        Ok(page("Unknown/Page")),
        Ok((StatusCode::OK, "<html></html>".into())),
        Err(anyhow::anyhow!("UNSAFE_REDIRECT")),
    ] {
        assert!(!job_level(&safe_code(&content_page(response).unwrap_err())));
    }
    assert_eq!(
        content_page(Ok(page("UrlRedirects/DownloadPage"))).unwrap()["props"]["purchase"]["id"],
        "authored-purchase"
    );
    // Transport failures, 5xx and 429 after their retries still stop the job.
    for code in ["NETWORK_UNAVAILABLE", "RATE_LIMITED"] {
        let error = content_page(Err(anyhow::anyhow!(code))).unwrap_err();
        assert!(job_level(&safe_code(&error)));
    }
    // Cookie-less public pages, file resolvers and signed file URLs never claim sign-in.
    for code in [
        "GUMROAD_PUBLIC_METADATA_UNAVAILABLE",
        "DOWNLOAD_RESOLVER_FAILED",
    ] {
        assert_eq!(
            asset_failure(anyhow::anyhow!("AUTH_REQUIRED"), code).to_string(),
            code
        );
        assert_eq!(
            asset_failure(anyhow::anyhow!("NETWORK_UNAVAILABLE"), code).to_string(),
            "NETWORK_UNAVAILABLE"
        );
    }
    for status in [401, 403] {
        let response = reqwest::Response::from(
            tauri::http::response::Builder::new()
                .status(status)
                .url(Url::parse("https://files.gumroad.com/authored/file.zip").unwrap())
                .body("")
                .unwrap(),
        );
        assert_eq!(
            validate_download_response(&response)
                .unwrap_err()
                .to_string(),
            "SIGNED_LINK_EXPIRED"
        );
    }
}

#[test]
fn product_local_failures_complete_the_job_and_count_distinct_assets() {
    let path =
        std::env::temp_dir().join(format!("nna-gumroad-incomplete-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = GumroadService::new(catalogue.clone()).unwrap();
    let account_id = "22222222-2222-4222-8222-222222222222";
    catalogue.put("account", account_id, json!({"id":account_id,"store":"Gumroad","state":"connected","status":"connected","identityVerified":true,"identityHash":"authored-buyer-hash","disconnected":false})).unwrap();
    let known = BTreeMap::from([
        (
            "authored-first-grant".to_owned(),
            "gumroad:authored-known".to_owned(),
        ),
        (
            "authored-second-grant".to_owned(),
            "gumroad:authored-known".to_owned(),
        ),
    ]);
    // Two failed grants of one product, a purchase new to the catalogue and a partial
    // inventory are three assets.
    let mut incomplete = BTreeSet::from(["gumroad:authored-partial".to_owned()]);
    for purchase in [
        "authored-first-grant",
        "authored-second-grant",
        "authored-new-purchase",
    ] {
        incomplete.insert(purchase_asset(&known, purchase));
    }
    assert_eq!(incomplete.len(), 3);
    // Their failures keep the known product blocked and membership unbound, so no
    // authoritative publication certifies around them.
    let job = json!({"id":"authored-incomplete-job","type":"gumroadSync","store":"Gumroad","accountId":account_id,"state":"running","progress":{"stage":"finalizing","incompleteProducts":9}});
    catalogue
        .put("job", "authored-incomplete-job", job.clone())
        .unwrap();
    catalogue.put("checkpoint", "authored-failed-page", json!({"id":"authored-failed-page","store":"Gumroad","accountId":account_id,"runId":job["id"],"observations":[{"purchaseId":"authored-first-grant","complete":false,"errorCode":"PERMISSION_DENIED"},{"purchaseId":"authored-new-purchase","complete":false,"errorCode":"GUMROAD_CONTENT_UNAVAILABLE"}],"records":[]})).unwrap();
    let checkpoint = json!({"id":"authored-checkpoint","journalIds":["authored-failed-page"]});
    let (_, blocked, unbound) =
        grouped_journals(&catalogue, &job, account_id, &checkpoint, &known).unwrap();
    assert!(blocked.contains("gumroad:authored-known") && unbound);
    let mut completed = job.clone();
    record_incomplete(&mut completed, &incomplete);
    service.finish_job(&completed, None).unwrap();
    let terminal = catalogue
        .get("job", "authored-incomplete-job")
        .unwrap()
        .unwrap();
    assert_eq!(terminal["state"], "completed");
    assert!(terminal["errorCode"].is_null());
    assert_eq!(terminal["progress"]["incompleteProducts"], 3);
    assert_eq!(
        catalogue.get("account", account_id).unwrap().unwrap()["state"],
        "connected"
    );
    let mut clean = job;
    record_incomplete(&mut clean, &BTreeSet::new());
    assert!(clean["progress"].get("incompleteProducts").is_none());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn membership_change_restarts_the_listing_once_and_order_never_fails_it() {
    fn page(count: u64, pages: u64, ids: &[&str]) -> LibraryPage {
        LibraryPage {
            buyer_identity: "authored-buyer".into(),
            pages,
            count,
            purchases: ids
                .iter()
                .map(|id| LibraryPurchase {
                    purchase_id: (*id).into(),
                    title: None,
                    author: None,
                    thumbnail_url: None,
                    native_type: Some("digital".into()),
                    variant_name: None,
                    archived: false,
                    content_route: None,
                })
                .collect(),
            bundles: vec![],
        }
    }
    let ids = |members: &BundleMembers| {
        members
            .purchases
            .iter()
            .map(|row| row.purchase_id.clone())
            .collect::<Vec<_>>()
    };
    // A purchase that moved onto the next page is listed once; nothing fails.
    let reordered = enumerate_bundle_members("authored-buyer", None, |index, archived| {
        async move {
            Ok(match (archived, index) {
                (true, _) => page(0, 1, &[]),
                (false, 1) => page(3, 2, &["first", "second"]),
                _ => page(3, 2, &["second", "third"]),
            })
        }
        .boxed()
    })
    .await
    .unwrap();
    assert_eq!(ids(&reordered), ["first", "second", "third"]);
    // A purchase that arrives mid-listing restarts it once from its first page.
    let calls = Arc::new(Mutex::new(0));
    let restarted = enumerate_bundle_members("authored-buyer", None, |index, archived| {
        let calls = calls.clone();
        async move {
            let call = {
                let mut calls = calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            Ok(match (archived, index) {
                (true, _) => page(0, 1, &[]),
                (false, 1) if call == 1 => page(3, 2, &["first", "second"]),
                (false, 1) => page(4, 2, &["new", "first"]),
                _ => page(4, 2, &["second", "third"]),
            })
        }
        .boxed()
    })
    .await
    .unwrap();
    assert_eq!(ids(&restarted), ["new", "first", "second", "third"]);
    assert_eq!(restarted.pages, 5);
    // A list that shrank past the page being read restarts it too.
    let calls = Arc::new(Mutex::new(0));
    let shrunk = enumerate_bundle_members("authored-buyer", None, |index, archived| {
        let calls = calls.clone();
        async move {
            let call = {
                let mut calls = calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            match (archived, index, call) {
                (true, _, _) => Ok(page(0, 1, &[])),
                (false, 1, 1) => Ok(page(3, 2, &["first", "second"])),
                (false, _, 2) => anyhow::bail!("GUMROAD_PAGINATION_UNPROVEN"),
                _ => Ok(page(2, 1, &["first", "second"])),
            }
        }
        .boxed()
    })
    .await
    .unwrap();
    assert_eq!(ids(&shrunk), ["first", "second"]);
    // Any other failure of a later page still stops the listing.
    let outage = enumerate_bundle_members("authored-buyer", None, |index, archived| {
        async move {
            match (archived, index) {
                (true, _) => Ok(page(0, 1, &[])),
                (false, 1) => Ok(page(3, 2, &["first", "second"])),
                _ => anyhow::bail!("NETWORK_UNAVAILABLE"),
            }
        }
        .boxed()
    })
    .await;
    assert_eq!(outage.err().unwrap().to_string(), "NETWORK_UNAVAILABLE");
    // A second change keeps the failure.
    let changing = enumerate_bundle_members("authored-buyer", None, |index, _| {
        async move { Ok(page(index + 2, 2, &["first"])) }.boxed()
    })
    .await;
    assert_eq!(
        changing.err().unwrap().to_string(),
        "GUMROAD_LIBRARY_CHANGED"
    );
    // A fresh first page keeps membership through reordering and pagination changes.
    let expected = signature(&page(4, 2, &["new", "first"]));
    let read = |id: &str| ["new", "first", "second", "third"].contains(&id);
    let reordered_head = page(4, 3, &["third", "new"]);
    assert!(membership_kept(&reordered_head, &expected, read));
    let grown_head = page(5, 2, &["new", "first"]);
    assert!(!membership_kept(&grown_head, &expected, read));
    let unread_head = page(4, 2, &["unread", "new"]);
    assert!(!membership_kept(&unread_head, &expected, read));
    let mut bundled = page(4, 2, &["new", "first"]);
    bundled.bundles.push(gumroad::LibraryBundle {
        product_id: "authored-bundle".into(),
        label: None,
    });
    assert!(!membership_kept(&bundled, &expected, read));
}
