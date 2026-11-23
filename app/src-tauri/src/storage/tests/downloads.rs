//! Download queueing, scheduling, cancellation and automatic downloads.
use super::*;

#[test]
fn download_scheduler_cas_does_not_revive_cancelled_queue_snapshots() {
    let f = Fixture::new();
    for action in ["require-auth", "clear-auth", "claim"] {
        let mut queued =
            json!({"id":"authored-scheduler","type":"download","state":"queued","fileIds":[]});
        if action == "clear-auth" {
            queued["stage"] = json!("authRequired");
            queued["errorCode"] = json!("auth_required");
        }
        f.service
            .catalogue
            .put("job", "authored-scheduler", queued.clone())
            .unwrap();
        f.service.cancel_job("authored-scheduler").unwrap();
        let cancelled = f
            .service
            .catalogue
            .get("job", "authored-scheduler")
            .unwrap()
            .unwrap();
        assert_eq!(cancelled["state"], "cancelled");
        let mut stale = queued.clone();
        let (state, error) = match action {
            "require-auth" => {
                stale["stage"] = json!("authRequired");
                ("queued", Some("auth_required"))
            }
            "clear-auth" => {
                stale.as_object_mut().unwrap().remove("stage");
                ("queued", None)
            }
            _ => ("running", None),
        };
        assert!(
            !f.service
                .put_queued_download(&queued, &mut stale, state, error)
                .unwrap(),
            "{action}"
        );
        assert_eq!(
            f.service
                .catalogue
                .get("job", "authored-scheduler")
                .unwrap()
                .unwrap(),
            cancelled,
            "{action}"
        );
    }
}

#[test]
fn download_scheduler_cas_claim_keeps_late_cancellation_and_queue_flags() {
    let f = Fixture::new();
    let queued = json!({"id":"authored-scheduler","type":"download","state":"queued","fileIds":[],"cancelRequested":true});
    f.service
        .catalogue
        .put("job", "authored-scheduler", queued.clone())
        .unwrap();
    let mut running = queued.clone();
    let token = f.service.cancellation("authored-scheduler");
    assert!(f
        .service
        .put_queued_download(&queued, &mut running, "running", None)
        .unwrap());
    assert_eq!(running["cancelRequested"], true);
    assert_eq!(
        f.service.cancel_job("authored-scheduler").unwrap()["cancelRequested"],
        true
    );
    assert!(token.load(Ordering::Acquire));
    let cancelled_running = f
        .service
        .catalogue
        .get("job", "authored-scheduler")
        .unwrap()
        .unwrap();
    assert_eq!(cancelled_running["state"], "running");
    assert_eq!(cancelled_running["cancelRequested"], true);
    let mut stale = queued.clone();
    assert!(!f
        .service
        .put_queued_download(&queued, &mut stale, "running", None)
        .unwrap());
    assert_eq!(
        f.service
            .catalogue
            .get("job", "authored-scheduler")
            .unwrap()
            .unwrap(),
        cancelled_running
    );
}

#[test]
fn cancellation_during_restore_and_after_completion_preserves_terminal_facts() {
    let f = Fixture::new();
    let running = json!({"id":"restore","type":"storage:restore_backup","state":"running"});
    let permit = f.service.catalogue.begin_restore_work().unwrap();
    f.service.catalogue.put("job", "restore", running).unwrap();
    assert_eq!(
        f.service.cancel_job("restore").unwrap()["cancelRequested"],
        true
    );
    assert!(f.service.cancellation("restore").load(Ordering::Acquire));
    assert_eq!(
        f.service.catalogue.get("job", "restore").unwrap().unwrap()["state"],
        "running"
    );
    let completed = json!({"id":"restore","type":"storage:restore_backup","state":"completed","result":{"restoredFiles":3}});
    f.service
        .catalogue
        .put("job", "restore", completed.clone())
        .unwrap();
    assert_eq!(
        f.service.cancel_job("restore").unwrap()["cancelRequested"],
        false
    );
    assert_eq!(
        f.service.catalogue.get("job", "restore").unwrap().unwrap(),
        completed
    );
    drop(permit);
}

#[tokio::test]
async fn cancel_downloads_stops_only_the_requested_products_active_jobs() {
    let f = Fixture::new();
    for (id, product, state) in [
        ("queued-a", "a", "queued"),
        ("running-a", "a", "running"),
        ("queued-b", "b", "queued"),
        ("done-a", "a", "completed"),
    ] {
        f.service
            .catalogue
            .put(
                "job",
                id,
                json!({"id":id,"type":"download","productId":product,"state":state,"fileIds":[]}),
            )
            .unwrap();
    }
    let running = f.service.cancellation("running-a");
    let job = |id: &str| f.service.catalogue.get("job", id).unwrap().unwrap();
    assert_eq!(
        f.service
            .cancel_downloads(&json!({"productIds":["a"]}))
            .await
            .unwrap()["cancelled"],
        2
    );
    assert_eq!(job("queued-a")["state"], "cancelled");
    assert_eq!(job("running-a")["state"], "running");
    assert_eq!(job("running-a")["cancelRequested"], true);
    assert!(running.load(Ordering::Acquire));
    assert_eq!(job("queued-b")["state"], "queued");
    assert_eq!(job("done-a")["state"], "completed");
    // Without products every remaining active download stops; a repeat changes nothing.
    assert_eq!(
        f.service.cancel_downloads(&json!({})).await.unwrap()["cancelled"],
        1
    );
    assert_eq!(job("queued-b")["state"], "cancelled");
    assert_eq!(
        f.service.cancel_downloads(&json!({})).await.unwrap()["cancelled"],
        0
    );
}

#[tokio::test]
async fn bulk_download_skips_products_without_files_and_one_explicit_product_reports_it() {
    let f = Fixture::new();
    f.seed(1, true);
    f.service
        .catalogue
        .put(
            "product",
            "43",
            json!({"id":"43","provider":"booth","title":"Authored empty"}),
        )
        .unwrap();
    f.service.catalogue.put("entitlement", "empty", json!({"id":"empty","productId":"43","accountId":"test-account","fileIds":[],"manifestComplete":true})).unwrap();
    let bulk = f
        .service
        .enqueue(&json!({"productIds":["42","43"]}))
        .await
        .unwrap();
    assert_eq!(bulk["jobIds"].as_array().unwrap().len(), 1);
    assert!(f.service.catalogue.related("job", "43").unwrap().is_empty());
    assert!(f
        .service
        .catalogue
        .get("download_intent", "43")
        .unwrap()
        .is_none());
    let single = f
        .service
        .enqueue(&json!({"productIds":["43"]}))
        .await
        .unwrap();
    let job = f
        .service
        .catalogue
        .get("job", single["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        (job["state"].as_str(), job["errorCode"].as_str()),
        (Some("needsAction"), Some("manifest_incomplete"))
    );
}

#[tokio::test]
async fn a_cancelling_download_does_not_absorb_an_explicit_new_request() {
    let f = Fixture::new();
    f.seed(1, true);
    let first = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    let first_id = first["jobIds"][0].as_str().unwrap();
    let mut running = f.service.catalogue.get("job", first_id).unwrap().unwrap();
    running["state"] = json!("running");
    f.service.catalogue.put("job", first_id, running).unwrap();
    f.service.cancel_job(first_id).unwrap();
    let next = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    assert_ne!(next["jobIds"][0], first["jobIds"][0]);
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 2);
}

#[tokio::test]
async fn stale_automatic_enqueue_rechecks_removed_or_disconnected_accounts_after_queue_pause() {
    for action in ["remove-keep", "disconnect", "remove-products"] {
        let f = Fixture::new();
        f.seed(1, true);
        let gate = f.service.pause_download_queue().await;
        let service = f.service.clone();
        let waiting = tokio::spawn(async move {
            service
                .enqueue(&json!({"productIds":["42"],"automatic":true}))
                .await
        });
        tokio::task::yield_now().await;
        assert!(
            !waiting.is_finished(),
            "existing queue gate must pause enqueue"
        );
        if action == "disconnect" {
            f.service.catalogue.put("account", "test-account", json!({"id":"test-account","store":"BOOTH","status":"disconnected","disconnected":true})).unwrap();
        } else {
            f.service
                .catalogue
                .remove_account("test-account", action == "remove-products")
                .unwrap();
        }
        drop(gate);
        assert_eq!(
            waiting.await.unwrap().unwrap()["jobIds"],
            json!([]),
            "{action}"
        );
        assert!(f.service.catalogue.list("job").unwrap().is_empty());
        assert!(f
            .service
            .catalogue
            .get("download_intent", "42")
            .unwrap()
            .is_none());
        if action == "remove-products" {
            assert_eq!(
                f.service
                    .enqueue(&json!({"productIds":["42"]}))
                    .await
                    .unwrap_err()
                    .to_string(),
                "product_missing"
            );
        } else {
            assert_eq!(
                f.service
                    .catalogue
                    .command("product_details", json!({"id":"42"}))
                    .unwrap()["status"],
                "notDownloaded"
            );
        }
        // The same ordinary automatic enqueue still works for a present enabled
        // account; stale suppression is not a global scheduling disable.
        f.seed(1, true);
        let result = f
            .service
            .enqueue(&json!({"productIds":["42"],"automatic":true}))
            .await
            .unwrap();
        assert_eq!(result["jobIds"].as_array().unwrap().len(), 1);
        assert_eq!(
            f.service.catalogue.list("job").unwrap()[0]["state"],
            "queued"
        );
        assert_eq!(
            f.service
                .catalogue
                .get("download_intent", "42")
                .unwrap()
                .unwrap()["wantDownload"],
            true
        );
    }
}

#[tokio::test]
async fn single_file_enqueue_is_bounded_and_preserves_product_intent_and_siblings() {
    let f = Fixture::new();
    f.seed(2, false);
    let retained = f.artifact(1);
    let original = io::safe_join(
        &f.path.join("library"),
        retained["relativePath"].as_str().unwrap(),
    )
    .unwrap();
    let result = f
        .service
        .enqueue_file(&json!({"fileId":"test-file-0","redownload":true}))
        .await
        .unwrap();
    let id = result["jobIds"][0].as_str().unwrap();
    let job = f.service.catalogue.get("job", id).unwrap().unwrap();
    assert_eq!(job["selectedFileIds"], json!(["test-file-0"]));
    assert_eq!(job["fileIds"], json!(["test-file-0"]));
    assert_eq!(job["redownload"], true);
    assert_eq!(job["manifestComplete"], false);
    assert!(f
        .service
        .catalogue
        .get("download_intent", "42")
        .unwrap()
        .is_none());
    assert_eq!(
        f.service
            .enqueue_file(&json!({"fileId":"test-file-0","redownload":true}))
            .await
            .unwrap()["jobIds"][0],
        id
    );
    let sibling_job = f
        .service
        .enqueue_file(&json!({"fileId":"test-file-1"}))
        .await
        .unwrap();
    assert_ne!(sibling_job["jobIds"][0], id);
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 2);
    assert_eq!(
        selected_download_files(
            f.service.remote_files("42").unwrap().0,
            &job["selectedFileIds"]
        )
        .unwrap()
        .len(),
        1
    );
    assert!(selected_download_files(
        f.service.remote_files("42").unwrap().0,
        &json!(["not-in-manifest"])
    )
    .is_err());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", retained["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        retained
    );
    io::verify(
        &original,
        retained["hash"].as_str().unwrap(),
        retained["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn independent_file_clicks_queue_behind_running_work_and_all_scope_reuses_coverage() {
    let f = Fixture::new();
    f.seed(3, true);
    let first_args = json!({"fileId":"test-file-0"});
    let second_args = json!({"fileId":"test-file-1"});
    let (first, second) = tokio::join!(
        f.service.enqueue_file(&first_args),
        f.service.enqueue_file(&second_args)
    );
    let first = first.unwrap()["jobIds"][0].as_str().unwrap().to_owned();
    let second = second.unwrap()["jobIds"][0].as_str().unwrap().to_owned();
    assert_ne!(first, second);
    let mut running = f.service.catalogue.get("job", &first).unwrap().unwrap();
    f.service.put_job(&mut running, "running", None).unwrap();
    assert_eq!(
        f.service
            .enqueue_file(&json!({"fileId":"test-file-0"}))
            .await
            .unwrap()["jobIds"],
        json!([first])
    );
    assert_eq!(
        f.service
            .enqueue_file(&json!({"fileId":"test-file-1"}))
            .await
            .unwrap()["jobIds"],
        json!([second])
    );
    let all = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    let all_id = all["jobIds"][0].as_str().unwrap();
    let all_job = f.service.catalogue.get("job", all_id).unwrap().unwrap();
    assert!(all_job["selectedFileIds"].is_null());
    assert_eq!(all_job["fileIds"].as_array().unwrap().len(), 3);
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":["42"]}))
            .await
            .unwrap(),
        all
    );
    assert_eq!(
        f.service
            .enqueue_file(&json!({"fileId":"test-file-2"}))
            .await
            .unwrap(),
        all
    );
    assert_eq!(
        f.service
            .enqueue_file(&json!({"fileId":"test-file-2","redownload":true}))
            .await
            .unwrap_err()
            .to_string(),
        "storage_busy"
    );
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 3);
    assert_eq!(
        f.service
            .catalogue
            .get("download_intent", "42")
            .unwrap()
            .unwrap()["wantDownload"],
        true
    );
}

#[test]
fn source_sync_reduces_only_matching_provider_launches_and_restores_saved_cap() {
    use crate::stores::StoreProvider;
    for (provider, sync, update) in [
        (StoreProvider::Booth, "boothSync", "boothUpdate"),
        (StoreProvider::Gumroad, "gumroadSync", "gumroadUpdate"),
        (StoreProvider::Jinxxy, "jinxxySync", "jinxxyUpdate"),
    ] {
        for kind in [sync, update] {
            let mut jobs = vec![json!({"type":kind,"state":"queued"})];
            assert_eq!(adaptive_download_count(&json!(6), Some(provider), &jobs), 1);
            jobs[0]["state"] = json!("running");
            assert_eq!(adaptive_download_count(&json!(4), Some(provider), &jobs), 1);
            for other in [
                StoreProvider::Booth,
                StoreProvider::Gumroad,
                StoreProvider::Jinxxy,
            ] {
                if other != provider {
                    assert_eq!(adaptive_download_count(&json!(4), Some(other), &jobs), 4);
                }
            }
            for state in ["completed", "failed", "cancelled", "needsAction"] {
                jobs[0]["state"] = json!(state);
                assert_eq!(adaptive_download_count(&json!(4), Some(provider), &jobs), 4);
            }
        }
    }
    let unrelated = vec![
        json!({"type":"boothLogin","state":"running"}),
        json!({"type":"download","store":"BOOTH","state":"running"}),
        json!({"type":"storage:import_local","state":"queued"}),
    ];
    assert_eq!(
        adaptive_download_count(&Value::Null, Some(StoreProvider::Booth), &unrelated),
        3
    );
}

#[test]
fn download_commit_cannot_overwrite_new_source_observation_and_retains_verified_bytes() {
    let f = Fixture::new();
    f.seed(1, true);
    let retained = f.artifact(0);
    let retained_path = io::safe_join(
        &f.path.join("library"),
        retained["relativePath"].as_str().unwrap(),
    )
    .unwrap();
    let mut observed = f
        .service
        .catalogue
        .get("remote_file", "test-file-0")
        .unwrap()
        .unwrap();
    observed["sourceObservationToken"] = json!("authored-sync-old");
    observed["requiresContentVerification"] = json!(true);
    f.service
        .catalogue
        .put("remote_file", "test-file-0", observed.clone())
        .unwrap();
    let pending_path = f.path.join("library/verified-new.bin");
    std::fs::write(&pending_path, b"verified actual transfer bytes").unwrap();
    let (hash, size) = io::hash_file(&pending_path).unwrap();
    let pending = json!({"id":"pending-authored-transfer","productId":"42","fileId":"test-file-0","rootId":"test-root","relativePath":"verified-new.bin","originalAvailable":true,"status":"downloaded","hash":hash,"sizeBytes":size});
    f.service
        .catalogue
        .put("artifact", "pending-authored-transfer", pending.clone())
        .unwrap();
    let mut finalized = observed.clone();
    finalized["revision"] = json!(format!("sha256:{hash}"));
    finalized["revisionBasis"] = json!("contentHash");
    finalized["requiresContentVerification"] = json!(false);
    finalized["sizeBytes"] = json!(size);
    let final_id = artifact_id(&finalized);
    let mut artifact = pending.clone();
    artifact["id"] = json!(final_id);
    artifact["revision"] = finalized["revision"].clone();
    let mut fresh = observed.clone();
    // A full refresh may retain the revision while advancing its observation
    // token. Comparing revision alone must not overwrite those new facts.
    fresh["sourceObservationToken"] = json!("authored-sync-new");
    f.service
        .catalogue
        .put("remote_file", "test-file-0", fresh.clone())
        .unwrap();
    let updates = vec![
        (
            "remote_file".into(),
            "test-file-0".into(),
            finalized.clone(),
        ),
        ("artifact".into(), final_id.clone(), artifact.clone()),
    ];
    let error = f
        .service
        .commit_download_records(
            updates.clone(),
            vec![("remote_file".into(), "test-file-0".into(), Some(observed))],
        )
        .unwrap_err();
    assert_eq!(io::stable_error(&error), "source_changed");
    assert_eq!(
        f.service
            .catalogue
            .get("remote_file", "test-file-0")
            .unwrap()
            .unwrap(),
        fresh
    );
    assert!(f
        .service
        .catalogue
        .get("artifact", &final_id)
        .unwrap()
        .is_none());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", "pending-authored-transfer")
            .unwrap()
            .unwrap(),
        pending
    );
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", retained["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        retained
    );
    io::verify(&pending_path, &hash, size).unwrap();
    io::verify(
        &retained_path,
        retained["hash"].as_str().unwrap(),
        retained["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    // A fresh attempt can commit the actual revision without erasing the new
    // source token, and leaves previously published bytes available.
    finalized["sourceObservationToken"] = fresh["sourceObservationToken"].clone();
    f.service
        .commit_download_records(
            vec![
                (
                    "remote_file".into(),
                    "test-file-0".into(),
                    finalized.clone(),
                ),
                ("artifact".into(), final_id.clone(), artifact),
            ],
            vec![("remote_file".into(), "test-file-0".into(), Some(fresh))],
        )
        .unwrap();
    assert_eq!(
        f.service
            .catalogue
            .get("remote_file", "test-file-0")
            .unwrap()
            .unwrap(),
        finalized
    );
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", &final_id)
            .unwrap()
            .unwrap()["originalAvailable"],
        true
    );
    io::verify(
        &retained_path,
        retained["hash"].as_str().unwrap(),
        retained["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn automatic_invalidation_survives_busy_and_offline_roots_without_repeated_full_scan() {
    let f = Fixture::new();
    f.seed(1, true);
    f.service
        .catalogue
        .command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    let operation = f.service.operations.write().await;
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    assert!(f
        .service
        .automatic_pending
        .lock()
        .unwrap()
        .products
        .contains("42"));
    drop(operation);
    std::fs::rename(f.path.join("library"), f.path.join("offline-library")).unwrap();
    f.service.reevaluate_automatic().await.unwrap();
    f.service.drain_automatic_changes().await.unwrap();
    assert!(f.service.automatic_pending.lock().unwrap().full);
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    std::fs::rename(f.path.join("offline-library"), f.path.join("library")).unwrap();
    f.service.drain_automatic_changes().await.unwrap();
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 1);
    let pending = f.service.automatic_pending.lock().unwrap();
    assert!(!pending.full && pending.products.is_empty());
}

#[tokio::test]
async fn automatic_source_delta_only_evaluates_touched_products_and_never_retries_same_failed_manifest(
) {
    let f = Fixture::new();
    f.seed(1, true);
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    f.service
        .catalogue
        .command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    f.service
        .catalogue
        .put(
            "product",
            "43",
            json!({"id":"43","title":"Other synthetic product"}),
        )
        .unwrap();
    f.service.catalogue.put("remote_file", "other-file", json!({"id":"other-file","productId":"43","accountId":"test-account","name":"other.psd","revision":"r1"})).unwrap();
    f.service.catalogue.put("entitlement", "other-entitlement", json!({"id":"other-entitlement","productId":"43","accountId":"test-account","fileIds":["other-file"],"manifestComplete":true})).unwrap();
    f.service
        .source_products_changed(&["42".into(), "42".into()])
        .await
        .unwrap();
    let mut jobs = f.service.catalogue.list("job").unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["productId"], "42");
    f.service
        .put_job(&mut jobs[0], "failed", Some("network_failed"))
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 1);
    let mut file = f
        .service
        .catalogue
        .get("remote_file", "test-file-0")
        .unwrap()
        .unwrap();
    file["revision"] = json!("r2");
    f.service
        .catalogue
        .put("remote_file", "test-file-0", file)
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    jobs = f.service.catalogue.list("job").unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|job| job["productId"] == "42"));
    f.service.reevaluate_automatic().await.unwrap();
    assert_eq!(f.service.catalogue.related("job", "43").unwrap().len(), 1);
}

#[tokio::test]
async fn automatic_delta_preserves_selected_intent_and_rechecks_updates_after_active_job() {
    let f = Fixture::new();
    f.seed(1, true);
    f.service
        .catalogue
        .command("preferences", json!({"patch":{"autoDownloadAll":false}}))
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    let first = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    let mut file = f
        .service
        .catalogue
        .get("remote_file", "test-file-0")
        .unwrap()
        .unwrap();
    file["revision"] = json!("r2");
    f.service
        .catalogue
        .put("remote_file", "test-file-0", file)
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 1);
    let mut job = f
        .service
        .catalogue
        .get("job", first["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    f.service.put_job(&mut job, "completed", None).unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    let jobs = f.service.catalogue.list("job").unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs
        .iter()
        .any(|job| job["state"] == "queued" && job["automatic"] == true));
}

#[tokio::test]
async fn clearing_activity_never_queues_a_product_download_again() {
    let f = Fixture::new();
    f.seed(1, true);
    let c = &f.service.catalogue;
    c.put(
        "product",
        "43",
        json!({"id":"43","title":"Other synthetic product"}),
    )
    .unwrap();
    c.put("remote_file","other-file",json!({"id":"other-file","productId":"43","accountId":"test-account","name":"other.psd","revision":"r1"})).unwrap();
    c.put("entitlement","other-entitlement",json!({"id":"other-entitlement","productId":"43","accountId":"test-account","fileIds":["other-file"],"manifestComplete":true})).unwrap();
    let queued = |product: &str| {
        c.related("job", product)
            .unwrap()
            .into_iter()
            .filter(|job| job["state"] == "queued")
            .count()
    };

    // Downloaded by an earlier version, whose intent kept no list, and Activity cleared.
    c.put(
        "download_intent",
        "42",
        json!({"id":"42","wantDownload":true}),
    )
    .unwrap();
    f.artifact(0);
    f.service.reevaluate_automatic().await.unwrap();
    assert_eq!(queued("42"), 0);

    // A failed or cancelled download waits for a retry however often Activity is cleared.
    for state in ["failed", "cancelled"] {
        let ids = f
            .service
            .enqueue(&json!({"productIds":["43"]}))
            .await
            .unwrap();
        let mut job = c
            .get("job", ids["jobIds"][0].as_str().unwrap())
            .unwrap()
            .unwrap();
        f.service.put_job(&mut job, state, None).unwrap();
        c.command("clear_activity", json!({})).unwrap();
        assert!(c.related("job", "43").unwrap().is_empty());
        f.service.reevaluate_automatic().await.unwrap();
        assert_eq!(queued("43"), 0);
    }

    // A job that met a changed list while it ran leaves that list behind.
    let ids = f
        .service
        .enqueue(&json!({"productIds":["43"]}))
        .await
        .unwrap();
    let mut file = c.get("remote_file", "other-file").unwrap().unwrap();
    file["revision"] = json!("r2");
    c.put("remote_file", "other-file", file).unwrap();
    let (files, _) = f.service.remote_files("43").unwrap();
    let mut job = c
        .get("job", ids["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    job["manifestFingerprint"] = json!(fingerprint(&files));
    f.service.remember_attempt(&job).unwrap();
    f.service
        .put_job(&mut job, "failed", Some("network_failed"))
        .unwrap();
    assert_eq!(
        c.get("download_intent", "43").unwrap().unwrap()["wantDownload"],
        true
    );
    c.command("clear_activity", json!({})).unwrap();
    f.service.reevaluate_automatic().await.unwrap();
    assert_eq!(queued("43"), 0);

    // A new revision of a downloaded file still queues, once.
    let mut file = c.get("remote_file", "test-file-0").unwrap().unwrap();
    file["revision"] = json!("r2");
    c.put("remote_file", "test-file-0", file).unwrap();
    f.service.reevaluate_automatic().await.unwrap();
    f.service.reevaluate_automatic().await.unwrap();
    assert_eq!(queued("42"), 1);
}

#[tokio::test]
async fn automatic_jobs_exclude_explicitly_disconnected_account_files() {
    let f = Fixture::new();
    f.seed(1, true);
    f.service
        .catalogue
        .put(
            "account",
            "removed-account",
            json!({"id":"removed-account","store":"BOOTH","disconnected":true}),
        )
        .unwrap();
    f.service.catalogue.put("remote_file","removed-file",json!({"id":"removed-file","productId":"42","accountId":"removed-account","name":"old.psd","revision":"r1"})).unwrap();
    f.service.catalogue.put("entitlement","removed-entitlement",json!({"id":"removed-entitlement","productId":"42","accountId":"removed-account","fileIds":["removed-file"],"manifestComplete":true})).unwrap();
    let queued = f
        .service
        .enqueue(&json!({"productIds":["42"],"automatic":true}))
        .await
        .unwrap();
    let job = f
        .service
        .catalogue
        .get("job", queued["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(job["fileIds"], json!(["test-file-0"]));
    assert_eq!(job["automatic"], true);
}

#[tokio::test]
async fn bulk_and_automatic_downloads_leave_out_ended_free_offers_until_asked_for_alone() {
    let f = Fixture::new();
    f.seed(2, true);
    let c = &f.service.catalogue;
    c.put(
        "product",
        "43",
        json!({"id":"43","title":"Other synthetic product"}),
    )
    .unwrap();
    c.put("remote_file","other-file",json!({"id":"other-file","productId":"43","accountId":"test-account","name":"other.psd","revision":"r1"})).unwrap();
    c.put("entitlement","other-entitlement",json!({"id":"other-entitlement","productId":"43","accountId":"test-account","fileIds":["other-file"],"manifestComplete":true})).unwrap();
    let end = |file: &str| {
        let id = format!("pending-{}", io::key(file));
        c.put("artifact",&id,json!({"id":id,"productId":"42","fileId":file,"revision":null,"status":"notDownloaded","errorCode":"free_offer_unavailable","originalAvailable":false})).unwrap();
    };
    let settle = || {
        for mut job in c.list("job").unwrap() {
            f.service.put_job(&mut job, "cancelled", None).unwrap();
        }
    };
    let jobs_of = |product: &str| {
        c.related("job", product)
            .unwrap()
            .into_iter()
            .filter(|job| job["state"] == "queued")
            .collect::<Vec<_>>()
    };
    let (listed, _) = f.service.remote_files("42").unwrap();

    // One ended file: the rest of the product still goes, and the job remembers why.
    end("test-file-0");
    f.service
        .enqueue(&json!({"productIds":["42","43"]}))
        .await
        .unwrap();
    let job = jobs_of("42").remove(0);
    assert_eq!(job["fileIds"], json!(["test-file-1"]));
    assert_eq!(job["skipEndedOffers"], true);
    assert_eq!(f.service.job_files(&job, listed.clone()).unwrap().len(), 1);
    assert_eq!(f.service.job_files(&json!({}), listed).unwrap().len(), 2);
    settle();

    // The job that found the offer closed held the whole list, and nothing more queues for it.
    c.command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    let (whole, _) = f.service.remote_files("42").unwrap();
    let mut failed = c.related("job", "42").unwrap().remove(0);
    failed["manifestFingerprint"] = json!(fingerprint(&whole));
    f.service
        .put_job(&mut failed, "needsAction", Some("free_offer_unavailable"))
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(jobs_of("42").is_empty());
    // Every file ended: no job, whether in a batch, automatic or after a source change.
    end("test-file-1");
    f.service
        .enqueue(&json!({"productIds":["42","43"]}))
        .await
        .unwrap();
    assert!(jobs_of("42").is_empty());
    assert_eq!(jobs_of("43").len(), 1);
    settle();
    let automatic = f
        .service
        .enqueue(&json!({"productIds":["42"],"automatic":true}))
        .await
        .unwrap();
    assert_eq!(automatic["jobIds"], json!([]));
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(jobs_of("42").is_empty());

    // Asking for this product alone tries every file again.
    f.service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    let job = jobs_of("42").remove(0);
    assert_eq!(job["fileIds"], json!(["test-file-0", "test-file-1"]));
    assert_eq!(job["skipEndedOffers"], false);
}

#[tokio::test]
async fn product_downloads_never_ask_again_for_a_file_its_store_refused() {
    let f = Fixture::new();
    f.seed(2, true);
    let c = &f.service.catalogue;
    let refuse = |file: &str| {
        let id = format!("pending-{}", io::key(file));
        c.put("artifact",&id,json!({"id":id,"productId":"42","fileId":file,"revision":null,"status":"notDownloaded","errorCode":"store_file_unavailable","originalAvailable":false})).unwrap();
    };
    let settle = || {
        for mut job in c.list("job").unwrap() {
            f.service.put_job(&mut job, "cancelled", None).unwrap();
        }
    };
    let queued = || {
        c.related("job", "42")
            .unwrap()
            .into_iter()
            .filter(|job| job["state"] == "queued")
            .collect::<Vec<_>>()
    };
    let (whole, _) = f.service.remote_files("42").unwrap();
    refuse("test-file-0");
    // Alone or automatically, a product download asks only for the rest.
    for args in [
        json!({"productIds":["42"]}),
        json!({"productIds":["42"],"automatic":true}),
    ] {
        f.service.enqueue(&args).await.unwrap();
        let job = queued().remove(0);
        assert_eq!(job["fileIds"], json!(["test-file-1"]));
        assert_eq!(f.service.job_files(&job, whole.clone()).unwrap().len(), 1);
        settle();
    }
    // That one file can still be asked for by itself.
    f.service
        .enqueue(&json!({"productIds":["42"],"selectedFileIds":["test-file-0"]}))
        .await
        .unwrap();
    assert_eq!(queued().remove(0)["fileIds"], json!(["test-file-0"]));
    settle();
    // The job that was refused held the whole list, so no automatic job follows it.
    for job in c.list("job").unwrap() {
        c.delete("job", job["id"].as_str().unwrap()).unwrap();
    }
    c.put("job","refused",json!({"id":"refused","type":"download","productId":"42","state":"needsAction","errorCode":"store_file_unavailable","selectedFileIds":null,"manifestFingerprint":fingerprint(&whole)})).unwrap();
    c.command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    f.service
        .source_products_changed(&["42".into()])
        .await
        .unwrap();
    assert!(queued().is_empty());
    // With every file refused there is nothing to ask for, and no job says the list is unknown.
    refuse("test-file-1");
    let alone = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    assert_eq!(alone["jobIds"], json!([]));
    assert!(queued().is_empty());
}

#[tokio::test]
async fn product_queue_includes_thirty_files_deduplicates_active_job_and_persists_intent() {
    let f = Fixture::new();
    f.seed(30, true);
    let first = f
        .service
        .enqueue(&json!({"productIds":["42"],"fileIds":["test-file-0"]}))
        .await
        .unwrap();
    let again = f
        .service
        .enqueue(&json!({"productIds":["42","42"]}))
        .await
        .unwrap();
    assert_eq!(first, again);
    let jobs = f.service.catalogue.list("job").unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["fileIds"].as_array().unwrap().len(), 30);
    assert_eq!(
        f.service
            .catalogue
            .get("download_intent", "42")
            .unwrap()
            .unwrap()["wantDownload"],
        true
    );
}

#[test]
fn entitlement_manifest_unknown_and_missing_metadata_are_not_complete() {
    let f = Fixture::new();
    f.seed(2, false);
    assert!(!f.service.remote_files("42").unwrap().1);
    let mut e = f
        .service
        .catalogue
        .get("entitlement", "test-entitlement")
        .unwrap()
        .unwrap();
    e["manifestComplete"] = json!(true);
    e["fileIds"]
        .as_array_mut()
        .unwrap()
        .push(json!("missing-metadata"));
    f.service
        .catalogue
        .put("entitlement", "test-entitlement", e)
        .unwrap();
    assert!(!f.service.remote_files("42").unwrap().1);
}

#[tokio::test]
async fn verified_files_survive_retry_and_partial_completion_requires_a_fault_for_attention() {
    let f = Fixture::new();
    f.seed(30, true);
    let mut saved = Vec::new();
    for i in 0..29 {
        saved.push(f.artifact(i));
    }
    let projection = || {
        let snapshot = f.service.catalogue.command("bootstrap", json!({})).unwrap();
        snapshot["products"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "42")
            .unwrap()
            .clone()
    };
    let product = projection();
    assert_eq!(product["status"], "partial");
    assert_eq!(product["downloaded"], 29);
    assert_eq!(product["total"], 30);
    assert_eq!(product["fullyDownloaded"], false);
    let queued = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    assert_eq!(projection()["status"], "processing");
    assert_eq!(projection()["fullyDownloaded"], false);
    let mut job = f
        .service
        .catalogue
        .get("job", queued["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    // Match a failed stream: pending bytes have no bound revision; the failed job
    // supplies Attention while all 29 completed artifact facts remain intact.
    let pending_id = format!("pending-{}", io::key("test-file-29"));
    f.service
        .catalogue
        .put(
            "artifact",
            &pending_id,
            json!({
                "id":pending_id,"productId":"42","fileId":"test-file-29",
                "rootId":"test-root","revision":null,"status":"notDownloaded",
                "originalAvailable":false,"errorCode":"network_failed"
            }),
        )
        .unwrap();
    f.service
        .put_job(&mut job, "failed", Some("network_failed"))
        .unwrap();
    assert_eq!(projection()["status"], "attention");
    assert_eq!(projection()["fullyDownloaded"], false);
    let retry = f
        .service
        .enqueue(&json!({"productIds":["42"]}))
        .await
        .unwrap();
    assert_ne!(retry, queued);
    assert_eq!(projection()["status"], "processing");
    for artifact in saved {
        assert_eq!(
            f.service
                .catalogue
                .get("artifact", artifact["id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
            artifact
        );
        assert!(f
            .service
            .artifact_verified(&artifact, &f.path.join("library"))
            .await
            .unwrap());
    }
    let mut retry_job = f
        .service
        .catalogue
        .get("job", retry["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    f.service
        .put_job(&mut retry_job, "cancelled", None)
        .unwrap();
    assert_eq!(projection()["status"], "partial");
    assert_eq!(projection()["fullyDownloaded"], false);
    // Every currently observed file must be Ready; source coverage is separate.
    f.artifact(29);
    let product = projection();
    assert_eq!(product["status"], "ready");
    assert_eq!(product["downloaded"], 30);
    assert_eq!(product["fullyDownloaded"], true);
    let mut entitlement = f
        .service
        .catalogue
        .get("entitlement", "test-entitlement")
        .unwrap()
        .unwrap();
    entitlement["manifestComplete"] = json!(false);
    f.service
        .catalogue
        .put("entitlement", "test-entitlement", entitlement)
        .unwrap();
    assert_eq!(projection()["status"], "ready");
    assert_eq!(projection()["manifestComplete"], false);
    assert_eq!(projection()["fullyDownloaded"], true);
}

#[tokio::test]
async fn network_wait_is_cancellable_without_waiting_for_timeout() {
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        signal.store(true, Ordering::Release);
    });
    let start = std::time::Instant::now();
    assert!(
        io::cancellable(&cancel, tokio::time::sleep(Duration::from_secs(3600)))
            .await
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(2));
}
