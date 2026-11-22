use super::*;
#[test]
fn malformed_inventory_detail_keeps_its_record_unresolved_and_later_grants_publish() {
    let path =
        std::env::temp_dir().join(format!("jinxxy-detail-isolation-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "d0140000-0000-4000-8000-000000000001";
    catalogue
        .put("account", account, json!({"id":account,"store":"Jinxxy"}))
        .unwrap();
    let entry = |inventory: &str, product: Option<&str>| InventoryEntry {
        inventory_id: inventory.into(),
        product_id: product.map(str::to_owned),
        order_id: None,
        created_at: None,
    };
    let malformed = entry("1234567890123456701", Some("1234567890123456711"));
    let healthy = entry("1234567890123456702", Some("1234567890123456712"));
    let file = json!({"id":"1234567890123456721","file_name":"authored.txt","size":1});
    let malformed_data = [
        json!({"inventory_item":{"id":malformed.inventory_id,"item":{"id":malformed.product_id,"files":[file.clone(),file.clone()]}}}),
    ];
    let healthy_data = [
        json!({"inventory_item":{"id":healthy.inventory_id,"item":{"id":healthy.product_id,"files":[file]}}}),
    ];
    let mut incomplete = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    let mut unbound = BTreeSet::new();
    assert!(sync_detail(
        &malformed_data,
        &malformed,
        &mut incomplete,
        &mut unresolved,
        &mut unbound
    )
    .is_none());
    let detail = sync_detail(
        &healthy_data,
        &healthy,
        &mut incomplete,
        &mut unresolved,
        &mut unbound,
    )
    .unwrap();
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_observed_group(&mut batch, account, &[(detail, None, None)]).unwrap();
    batch.commit().unwrap();
    assert!(incomplete.contains("1234567890123456711"));
    assert!(!incomplete.contains("1234567890123456712"));
    assert!(unresolved.contains("1234567890123456701"));
    assert!(unbound.is_empty());
    assert!(catalogue
        .related("acquisition", "jinxxy:1234567890123456711")
        .unwrap()
        .is_empty());
    assert_eq!(
        catalogue
            .related("acquisition", "jinxxy:1234567890123456712")
            .unwrap()
            .len(),
        1
    );
    let unknown = entry("1234567890123456701", None);
    assert!(sync_detail(
        &malformed_data,
        &unknown,
        &mut incomplete,
        &mut unresolved,
        &mut unbound
    )
    .is_none());
    // An unbound failed grant blocks every membership certification.
    assert_eq!(unbound, BTreeSet::from(["1234567890123456701".to_owned()]));
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn one_assets_definitive_answer_leaves_it_unresolved_but_outages_still_fail_the_job() {
    for code in [
        "AUTH_REQUIRED",
        "PERMISSION_DENIED",
        "UNSAFE_REDIRECT",
        "JINXXY_SOURCE_UNAVAILABLE",
        "METADATA_TOO_LARGE",
        "METADATA_ENCODING",
        "JINXXY_METADATA_TOO_LARGE",
        "JINXXY_TRANSPORT_UNSUPPORTED",
        "JINXXY_SOURCE_UNSUPPORTED",
    ] {
        assert!(asset_answer(&anyhow::anyhow!(code)), "{code}");
    }
    for code in [
        "NETWORK_UNAVAILABLE",
        "RATE_LIMITED",
        "HTTP_CLIENT_FAILED",
        "INVALID_METADATA_URL",
        "CANCELLED",
        "CONCURRENT_UPDATE",
    ] {
        assert!(!asset_answer(&anyhow::anyhow!(code)), "{code}");
    }
    let entry = |inventory: &str, product: Option<&str>| InventoryEntry {
        inventory_id: inventory.into(),
        product_id: product.map(str::to_owned),
        order_id: None,
        created_at: None,
    };
    let (mut incomplete, mut unresolved, mut unbound) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for (inventory, product) in [
        ("1234567890123456701", Some("1234567890123456711")),
        ("1234567890123456702", Some("1234567890123456711")),
        ("1234567890123456703", None),
    ] {
        unresolved_detail(
            &entry(inventory, product),
            &mut incomplete,
            &mut unresolved,
            &mut unbound,
        );
    }
    assert_eq!(unresolved.len(), 3);
    // Two grants of one product count once; a grant without a known product counts alone.
    let sync = json!({"type":"jinxxySync"});
    assert_eq!(
        incomplete_assets(&sync, &incomplete, &unbound, &BTreeSet::new()),
        2
    );
    assert_eq!(
        incomplete_assets(&sync, &BTreeSet::new(), &BTreeSet::new(), &BTreeSet::new()),
        0
    );
    // A selected update counts its selected products the listing no longer shows.
    let update = json!({"type":"jinxxyUpdate","productIds":["jinxxy:1234567890123456711","jinxxy:1234567890123456712"]});
    let missing =
        missing_selected(&update, &BTreeSet::from(["1234567890123456711".to_owned()])).unwrap();
    assert_eq!(missing, BTreeSet::from(["1234567890123456712".to_owned()]));
    assert_eq!(
        incomplete_assets(&update, &incomplete, &unbound, &missing),
        2
    );
}

#[test]
fn listing_change_detection_counts_only_grant_membership() {
    let listing = |rows: Value, pages: u64| {
        parse_inventory(
            &[json!({"inventory_items":{"page":1,"page_count":pages,"payload":rows}})],
            1,
        )
        .unwrap()
    };
    let first = json!({"id":INVENTORY,"created_at":"2026-01-01T00:00:00Z","item":{"id":PRODUCT,"name":"Authored title","url":"authored","base_price":1000,"currency_code":"USD","description":"Authored description","cover":{"url":"https://jinxxy-c.jinxxy-cdn.com/authored.png"},"user":{"username":"authored-maker"}}});
    let second = json!({"id":"1234567890123456790","item":{"id":"1234567890123456791"}});
    let base = membership_signature(&listing(json!([first.clone(), second.clone()]), 2));
    // Seller edits and a new order are not membership changes.
    let mut edited = first.clone();
    edited["item"]["name"] = json!("Renamed by the seller");
    edited["item"]["url"] = json!("renamed-slug");
    edited["item"]["base_price"] = json!(2500);
    edited["item"]["description"] = json!("Rewritten description");
    edited["item"]["cover"]["url"] = json!("https://jinxxy-c.jinxxy-cdn.com/replaced.png");
    edited["item"]["user"]["username"] = json!("renamed-maker");
    assert_eq!(
        membership_signature(&listing(json!([second.clone(), edited]), 2)),
        base
    );
    // A new or removed grant, a rebound product or a new page count is one.
    let third = json!({"id":"1234567890123456792","item":{"id":"1234567890123456793"}});
    assert_ne!(
        membership_signature(&listing(json!([first.clone(), second.clone(), third]), 2)),
        base
    );
    assert_ne!(
        membership_signature(&listing(json!([first.clone()]), 2)),
        base
    );
    let mut rebound = second.clone();
    rebound["item"]["id"] = json!("1234567890123456799");
    assert_ne!(
        membership_signature(&listing(json!([first.clone(), rebound]), 2)),
        base
    );
    assert_ne!(
        membership_signature(&listing(json!([first, second]), 3)),
        base
    );
}

#[test]
fn sign_in_signal_keeps_the_protected_session_and_only_marks_the_account() {
    let fixture = WorkerFixture::new();
    let service = fixture.service();
    let catalogue = service.inner.catalogue.clone();
    assert!(catalogue.library_root().is_some());
    let account = "d0140000-0000-4000-8000-000000000001";
    let record = json!({"id":account,"store":"Jinxxy","status":"connected","state":"connected","identityVerified":true,"identityHash":"authored-identity"});
    let snapshot = session::profile_path(&catalogue.device_dir(), StoreProvider::Jinxxy, account)
        .unwrap()
        .join("session.bin");
    std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
    std::fs::write(&snapshot, b"authored protected snapshot").unwrap();
    for kind in ["jinxxySync", "jinxxyUpdate"] {
        catalogue.put("account", account, record.clone()).unwrap();
        let mut job =
            json!({"id":kind,"type":kind,"store":"Jinxxy","accountId":account,"state":"running"});
        catalogue.put("job", kind, job.clone()).unwrap();
        service
            .finish_job(&mut job, "needsAction", Some("AUTH_REQUIRED"))
            .unwrap();
        assert_eq!(
            std::fs::read(&snapshot).unwrap(),
            b"authored protected snapshot"
        );
        // The shared record stays as it was for other devices; this device waits for a
        // sign-in, whose save clears its marker.
        let stored = catalogue.get("account", account).unwrap().unwrap();
        for field in ["status", "state", "identityHash", "identityVerified"] {
            assert_eq!(stored[field], record[field]);
        }
        let profile = snapshot.parent().unwrap();
        assert!(super::super::session_cache::signed_out(profile));
        super::super::session_cache::clear(profile).unwrap();
        assert!(!super::super::session_cache::signed_out(profile));
        std::fs::write(&snapshot, b"authored protected snapshot").unwrap();
    }
    drop(catalogue);
    drop(service);
    fixture.finish();
}

#[test]
fn startup_recovery_isolates_orphan_jobs_and_keeps_later_jobs_resumable() {
    let path = std::env::temp_dir().join(format!("jinxxy-startup-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let missing = "d0140000-0000-4000-8000-000000000001";
    let healthy = "d0140000-0000-4000-8000-000000000002";
    catalogue
        .put("account", healthy, json!({"id":healthy,"store":"Jinxxy"}))
        .unwrap();
    for (id, owner) in [
        ("authored-orphan-job", missing),
        ("authored-healthy-job", healthy),
    ] {
        let job = json!({"id":id,"type":"jinxxySync","store":"Jinxxy","accountId":owner,"state":"running"});
        catalogue.put("job", id, job.clone()).unwrap();
        service.prepare_recovered_job(job).unwrap();
    }
    let orphan = catalogue
        .get("job", "authored-orphan-job")
        .unwrap()
        .unwrap();
    assert_eq!(orphan["state"], "needsAction");
    assert_eq!(orphan["errorCode"], "ACCOUNT_UNAVAILABLE");
    assert_eq!(
        catalogue
            .get("job", "authored-healthy-job")
            .unwrap()
            .unwrap()["state"],
        "queued"
    );
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn queued_login_is_selected_independently_and_accounts_are_reserved_once() {
    let fixture = WorkerFixture::new();
    let service = fixture.service();
    let first = "d0140000-0000-4000-8000-000000000001";
    let second = "d0140000-0000-4000-8000-000000000002";
    for (id, kind, account) in [
        ("authored-metadata", "jinxxySync", first),
        ("authored-login", "jinxxyLogin", second),
        ("authored-same-account", "jinxxyUpdate", first),
    ] {
        service
            .inner
            .catalogue
            .put(
                "job",
                id,
                json!({"id":id,"type":kind,"store":"Jinxxy","accountId":account,"state":"queued"}),
            )
            .unwrap();
    }
    let metadata_busy = service.inner.execution.try_lock().unwrap();
    let owner = Arc::downgrade(&service.inner);
    let (login_owner, login, login_work) = JinxxyService::next_worker_job(&owner).unwrap();
    assert_eq!(login["type"], "jinxxyLogin");
    let (metadata_owner, metadata, metadata_work) = JinxxyService::next_worker_job(&owner).unwrap();
    assert_eq!(metadata["accountId"], first);
    assert!(JinxxyService::next_worker_job(&owner).is_none());
    assert_eq!(service.inner.running.lock().unwrap().len(), 2);
    drop(login_work);
    drop(metadata_work);
    drop(login_owner);
    drop(metadata_owner);
    drop(metadata_busy);
    drop(service);
    assert!(owner.upgrade().is_none());
    fixture.finish();
}

#[test]
fn retry_cannot_overwrite_a_new_attempt_or_reserved_account() {
    let fixture = WorkerFixture::new();
    let service = fixture.service();
    let before = json!({"id":"authored-retry","type":"jinxxySync","store":"Jinxxy","accountId":"authored-account","state":"failed","cancelRequested":true});
    service
        .inner
        .catalogue
        .put("job", "authored-retry", before.clone())
        .unwrap();
    service.ensure_retryable(&before).unwrap();
    service
        .inner
        .running
        .lock()
        .unwrap()
        .insert(JinxxyService::worker_key(&before));
    assert!(service.requeue_job(&before).is_err());
    assert_eq!(
        service
            .inner
            .catalogue
            .get("job", "authored-retry")
            .unwrap(),
        Some(before.clone())
    );
    service.inner.running.lock().unwrap().clear();
    let queued = service.requeue_job(&before).unwrap();
    assert_eq!(queued["state"], "queued");
    assert_eq!(queued["cancelRequested"], false);
    assert!(service.requeue_job(&before).is_err());
    assert!(service.ensure_retryable(&queued).is_err());
    assert_eq!(
        service
            .inner
            .catalogue
            .get("job", "authored-retry")
            .unwrap(),
        Some(queued)
    );
}
#[test]
fn incomplete_inventory_grant_keeps_known_files_and_does_not_block_healthy_publication() {
    let path = std::env::temp_dir().join(format!(
        "nna-jinxxy-partial-grants-{}",
        uuid::Uuid::new_v4()
    ));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let account = "d0140000-0000-4000-8000-000000000001";
    let partial_inventory = "1234567890123456701";
    let healthy_inventory = "1234567890123456702";
    let partial_product = "1234567890123456711";
    let healthy_product = "1234567890123456712";
    catalogue
        .put(
            "account",
            account,
            json!({"id":account,"store":"Jinxxy","status":"connected","state":"connected"}),
        )
        .unwrap();
    let partial_entitlement = format!("jinxxy:{account}:{partial_inventory}");
    catalogue.put("entitlement", &partial_entitlement, json!({"id":partial_entitlement,"accountId":account,"productId":format!("jinxxy:{partial_product}"),"inventoryId":partial_inventory,"source":"library","fileIds":["retained-file"],"manifestComplete":false})).unwrap();
    let partial = parse_detail(&[json!({"inventory_item":{"id":partial_inventory,"item":{"id":partial_product,"name":"Authored incomplete","files":[]}}})], partial_inventory, Some(partial_product)).unwrap();
    let healthy = parse_detail(&[json!({"inventory_item":{"id":healthy_inventory,"item":{"id":healthy_product,"name":"Authored healthy","files":[{"id":"1234567890123456721","file_name":"healthy.txt","size":1}]}}})], healthy_inventory, Some(healthy_product)).unwrap();
    assert!(!partial.inventory_complete);
    assert!(healthy.inventory_complete);
    for detail in [partial, healthy] {
        let mut batch = crate::booth::RecordBatch::new(&catalogue);
        stage_observed_group(&mut batch, account, &[(detail, None, None)]).unwrap();
        batch.commit().unwrap();
    }
    assert_eq!(
        catalogue
            .get("entitlement", &partial_entitlement)
            .unwrap()
            .unwrap()["fileIds"],
        json!(["retained-file"])
    );
    let mut job = json!({"id":"authored-partial-job","type":"jinxxySync","store":"Jinxxy","accountId":account,"state":"running","completedProducts":2});
    catalogue
        .put("job", "authored-partial-job", job.clone())
        .unwrap();
    service
        .certify_product_membership(
            &job,
            account,
            &format!("jinxxy:{healthy_product}"),
            &BTreeSet::from([healthy_inventory.to_owned()]),
        )
        .unwrap();
    assert_eq!(
        catalogue
            .get(
                "entitlement",
                &format!("jinxxy:{account}:{healthy_inventory}")
            )
            .unwrap()
            .unwrap()["manifestComplete"],
        true
    );
    assert_eq!(
        catalogue
            .get("entitlement", &partial_entitlement)
            .unwrap()
            .unwrap()["manifestComplete"],
        false
    );
    // The partial grant no longer holds every sync at needs-action; the job completes and counts it.
    let incomplete = incomplete_assets(
        &job,
        &BTreeSet::from([partial_product.to_owned()]),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    job["progress"] = json!({"stage":"finalizing","incompleteProducts":incomplete});
    service.finish_job(&mut job, "completed", None).unwrap();
    assert_eq!(
        (&job["state"], &job["errorCode"]),
        (&json!("completed"), &Value::Null)
    );
    assert_eq!(job["progress"]["incompleteProducts"], 1);
    let stored = catalogue
        .get("job", "authored-partial-job")
        .unwrap()
        .unwrap();
    assert_eq!(
        (&stored["state"], &stored["progress"]["incompleteProducts"]),
        (&json!("completed"), &json!(1))
    );
    let account_row = catalogue.get("account", account).unwrap().unwrap();
    assert_eq!(
        (&account_row["status"], &account_row["syncState"]),
        (&json!("connected"), &json!("complete"))
    );
    let partial_grant = catalogue
        .get("entitlement", &partial_entitlement)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            &partial_grant["manifestComplete"],
            &partial_grant["fileIds"]
        ),
        (&json!(false), &json!(["retained-file"]))
    );
    assert_eq!(
        catalogue
            .related("acquisition", &format!("jinxxy:{healthy_product}"))
            .unwrap()
            .len(),
        1
    );
    // An asset left as it was keeps coverage open, as in the other stores.
    let coverage = catalogue
        .get("source_coverage", &format!("jinxxy:{account}:inventory"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (&coverage["status"], &coverage["coverageComplete"]),
        (&json!("incomplete"), &json!(false))
    );
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

struct WorkerFixture(std::path::PathBuf);
impl WorkerFixture {
    fn new() -> Self {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/verification/jinxxy-worker-owner")
            .join(format!("authored-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("library")).unwrap();
        Self(root)
    }
    fn service(&self) -> JinxxyService {
        let catalogue =
            Catalogue::open_bound(&self.0.join("library"), &self.0.join("device")).unwrap();
        JinxxyService::new(catalogue).unwrap()
    }
    fn finish(self) {
        let boundary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/verification/jinxxy-worker-owner")
            .canonicalize()
            .unwrap();
        crate::storage::validate_library_path(&self.0).unwrap();
        let root = self.0.canonicalize().unwrap();
        assert_eq!(root.parent(), Some(boundary.as_path()));
        assert!(uuid::Uuid::parse_str(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .strip_prefix("authored-")
                .unwrap()
        )
        .is_ok());
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            crate::storage::validate_library_path(&path).unwrap();
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn idle_worker_releases_previous_library_when_services_are_replaced() {
    let fixture = WorkerFixture::new();
    let service = fixture.service();
    let root = service
        .inner
        .catalogue
        .library_root()
        .unwrap()
        .to_path_buf();
    let owner = Arc::downgrade(&service.inner);
    assert!(JinxxyService::next_worker_job(&owner).is_none());
    assert_eq!(owner.strong_count(), 1);
    let switching = service.inner.catalogue.begin_restore_work().unwrap();
    assert!(JinxxyService::next_worker_job(&owner).is_none());
    drop(switching);
    drop(service);
    assert!(owner.upgrade().is_none());
    assert!(JinxxyService::next_worker_job(&owner).is_none());
    let released = crate::library_lock::LibraryLock::acquire(&root).unwrap();
    drop(released);
    let reopened = Catalogue::open_bound(&root, &fixture.0.join("device")).unwrap();
    drop(reopened);
    fixture.finish();
}
#[test]
fn selected_worker_job_retains_library_and_excludes_switch_until_finished() {
    let fixture = WorkerFixture::new();
    let service = fixture.service();
    let root = service
        .inner
        .catalogue
        .library_root()
        .unwrap()
        .to_path_buf();
    for (id, store) in [("authored-other", "BOOTH"), ("authored-selected", "Jinxxy")] {
        service
            .inner
            .catalogue
            .put(
                "job",
                id,
                json!({"id":id,"type":"sync","store":store,"state":"queued"}),
            )
            .unwrap();
    }
    let owner = Arc::downgrade(&service.inner);
    let (running, mut job, work) = JinxxyService::next_worker_job(&owner).unwrap();
    assert_eq!(job["id"], "authored-selected");
    drop(service);
    assert!(owner.upgrade().is_some());
    assert!(crate::library_lock::LibraryLock::acquire(&root).is_err());
    assert_eq!(
        running
            .inner
            .catalogue
            .begin_restore_work()
            .err()
            .unwrap()
            .to_string(),
        "storage_busy"
    );
    job["state"] = json!("completed");
    running
        .inner
        .catalogue
        .put("job", "authored-selected", job)
        .unwrap();
    running
        .inner
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    // The live permit still protects terminal cleanup even after queued
    // records are gone. Releasing it permits the in-process switch.
    running
        .inner
        .catalogue
        .delete("job", "authored-other")
        .unwrap();
    assert!(running.inner.catalogue.begin_restore_work().is_err());
    drop(work);
    let switching = running.inner.catalogue.begin_restore_work().unwrap();
    drop(switching);
    drop(running);
    assert!(owner.upgrade().is_none());
    let released = crate::library_lock::LibraryLock::acquire(&root).unwrap();
    drop(released);
    fixture.finish();
}
#[test]
fn terminal_result_does_not_recreate_a_removed_job() {
    let path =
        std::env::temp_dir().join(format!("nna-jinxxy-removed-job-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let account_id = "d0140000-0000-4000-8000-000000000001";
    let mut job = json!({"id":"removed-job","type":"jinxxySync","store":"Jinxxy","accountId":account_id,"state":"running"});
    assert_eq!(
        service
            .finish_job(&mut job, "completed", None)
            .unwrap_err()
            .to_string(),
        "JOB_UNAVAILABLE"
    );
    assert!(catalogue.get("job", "removed-job").unwrap().is_none());
    assert!(catalogue.list("source_coverage").unwrap().is_empty());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn auth_required_expires_account_without_erasing_verified_ownership() {
    let path = std::env::temp_dir().join(format!("nna-jinxxy-auth-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let account_id = "d0140000-0000-4000-8000-000000000001";
    let account = json!({"id":account_id,"store":"Jinxxy","status":"connected","state":"connected","identityVerified":true,"identityHash":"authored-identity","authenticatedAt":"2026-01-01T00:00:00Z","lastSyncAt":"2026-01-02T00:00:00Z"});
    let entitlement =
        json!({"id":"authored-entitlement","accountId":account_id,"productId":"authored-product"});
    catalogue
        .put("entitlement", "authored-entitlement", entitlement.clone())
        .unwrap();
    for kind in ["jinxxySync", "jinxxyUpdate"] {
        catalogue
            .put("account", account_id, account.clone())
            .unwrap();
        let mut job = json!({"id":kind,"type":kind,"store":"Jinxxy","accountId":account_id,"state":"running"});
        catalogue.put("job", kind, job.clone()).unwrap();
        catalogue
            .command(
                "preferences",
                json!({"patch":{"avatarAutomaticEnabled":{"booth:3390957":true}}}),
            )
            .unwrap();
        service
            .finish_job(&mut job, "needsAction", Some("AUTH_REQUIRED"))
            .unwrap();
        assert_eq!(
            catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
            json!({})
        );
        let expired = catalogue.get("account", account_id).unwrap().unwrap();
        assert_eq!(expired["status"], "expired");
        assert_eq!(expired["state"], "expired");
        for key in [
            "identityVerified",
            "identityHash",
            "authenticatedAt",
            "lastSyncAt",
        ] {
            assert_eq!(expired[key], account[key]);
        }
        assert_eq!(
            catalogue
                .get("entitlement", "authored-entitlement")
                .unwrap()
                .unwrap(),
            entitlement
        );
        catalogue
            .put("account", account_id, account.clone())
            .unwrap();
        catalogue
            .command(
                "preferences",
                json!({"patch":{"avatarAutomaticEnabled":{"booth:3390957":true}}}),
            )
            .unwrap();
        service
            .finish_job(&mut job, "failed", Some("NETWORK_UNAVAILABLE"))
            .unwrap();
        assert_eq!(
            catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
            json!({})
        );
        assert_eq!(
            catalogue.get("account", account_id).unwrap().unwrap()["status"],
            "connected"
        );
        catalogue
            .command(
                "preferences",
                json!({"patch":{"avatarAutomaticEnabled":{"booth:3390957":true}}}),
            )
            .unwrap();
        service.finish_job(&mut job, "completed", None).unwrap();
        assert_eq!(job["state"], "completed");
        assert_eq!(
            catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
            json!({})
        );
        let mut cancellation = job.clone();
        cancellation["state"] = json!("running");
        cancellation["cancelRequested"] = json!(true);
        catalogue.put("job", kind, cancellation).unwrap();
        catalogue
            .command(
                "preferences",
                json!({"patch":{"avatarAutomaticEnabled":{"booth:3390957":true}}}),
            )
            .unwrap();
        service.finish_job(&mut job, "completed", None).unwrap();
        assert_eq!(
            job["state"], "cancelled",
            "Durable cancellation wins over a stale successful job"
        );
        assert_eq!(job["errorCode"], "CANCELLED");
        assert_eq!(
            catalogue.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
            json!({})
        );
    }
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
const INVENTORY: &str = "111111111111111111";
const PRODUCT: &str = "222222222222222222";
const FILE: &str = "333333333333333333";
fn script(value: &Value) -> String {
    format!(
        "<script>(window[Symbol.for(\"ApolloSSRDataTransport\")] ??= []).push({value});</script>"
    )
}
#[test]
fn streaming_ready_data_is_found_after_undefined_and_private_fields_are_discarded() {
    let loading = "<script>(window[Symbol.for(\"ApolloSSRDataTransport\")] ??= []).push({\"rehydrate\":{\"q\":{\"data\":undefined}}});</script>";
    let ready = json!({"events":[{"value":{"data":{"inventory_item":{"id":INVENTORY,"license":"AUTHORED_SECRET","seller_support":"AUTHORED_CONTACT","item":{"id":PRODUCT,"name":"undefined \\\" } remains text","files":[{"id":FILE,"file_name":"all-files.zip","size":0,"file_meta":{"secret":"AUTHORED_SECRET"}}],"user":{"username":"Fixture creator","email":"AUTHORED_CONTACT"}}},"viewer":{"token":"AUTHORED_SECRET"}}}}]});
    let data = buyer_data(&format!("{loading}{}", script(&ready))).unwrap();
    assert!(!serde_json::to_string(&data).unwrap().contains("AUTHORED_"));
    let parsed = parse_detail(&data, INVENTORY, Some(PRODUCT)).unwrap();
    assert!(parsed.inventory_complete);
    assert_eq!(parsed.files[0].size_bytes, Some(0));
    assert_eq!(
        parsed.title.as_deref(),
        Some("undefined \\\" } remains text")
    );
    assert!(parse_detail(&data, INVENTORY, Some(FILE)).is_err());
}
#[test]
fn transport_rejects_executable_values_and_ignores_non_query_cache() {
    assert!(transport_objects(
        "(window[Symbol.for(\"ApolloSSRDataTransport\")] ??= []).push({\"data\":evil()});"
    )
    .is_err());
    assert!(transport_objects(
        "(window[Symbol.for(\"ApolloSSRDataTransport\")] ??= []).push({});evil();"
    )
    .is_err());
    assert!(buyer_data(&script(
        &json!({"cache":{"inventory_item":{"id":INVENTORY}}})
    ))
    .is_err());
    assert_eq!(
        normalize_undefined("[undefined,\"undefined\",{\"undefined\":undefined}]"),
        "[null,\"undefined\",{\"undefined\":null}]"
    );
}
#[test]
fn buyer_identity_binds_protected_profile_navigation_without_seller_or_profile_details() {
    let html="<nav><a href='/authored_buyer' class='block'>Profile</a></nav><a href='/authored_seller'>Profile</a>";
    let profile = protected_profile_route(html).unwrap();
    let user = json!({"id":"4444444444444444444","username":"authored_buyer","bio":"AUTHORED_PRIVATE","email":"AUTHORED_PRIVATE"});
    let data=buyer_data(&script(&json!({"events":[{"value":{"data":{"user":user.clone()}}},{"value":{"data":{"user":user}}}]}))).unwrap();
    assert_eq!(
        profile_identity(&data, &profile).unwrap(),
        "4444444444444444444"
    );
    let localized_profile = protected_profile_route(
        "<nav><a href='https://zh.jinxxy.com/authored_buyer'>Profile</a></nav>",
    )
    .unwrap();
    assert_eq!(localized_profile, profile);
    assert_eq!(
        profile_identity(&data, &localized_profile).unwrap(),
        "4444444444444444444"
    );
    let chinese_profile = protected_profile_route(
        "<nav><a href='https://zh.jinxxy.com/authored_buyer'>个人资料</a></nav>",
    )
    .unwrap();
    assert_eq!(chinese_profile, profile);
    assert_eq!(
        profile_identity(&data, &chinese_profile).unwrap(),
        "4444444444444444444"
    );
    assert!(protected_profile_route(
        "<nav><a href='https://other.example/authored_buyer'>个人资料</a></nav>"
    )
    .is_err());
    // Any Jinxxy host may carry the link; the route is always read from the apex.
    for host in ["ja.jinxxy.com", "www.jinxxy.com", "app.jinxxy.com"] {
        assert_eq!(
            protected_profile_route(&format!(
                "<nav><a href='https://{host}/authored_buyer'>Profile</a></nav>"
            ))
            .unwrap(),
            profile
        );
    }
    assert!(!serde_json::to_string(&data)
        .unwrap()
        .contains("AUTHORED_PRIVATE"));
    assert!(profile_identity(
        &[json!({"user":{"id":PRODUCT,"username":"authored_seller"}})],
        &profile
    )
    .is_err());
    let mut switched = data.clone();
    switched.push(json!({"user":{"id":PRODUCT,"username":"authored_buyer"}}));
    assert!(profile_identity(&switched, &profile).is_err());
    for href in [
        "https://other.example/authored_buyer",
        "https://zh.jinxxy.com.example/authored_buyer",
        "https://evil-jinxxy.com/authored_buyer",
        "https://127.0.0.1/authored_buyer",
        "http://zh.jinxxy.com/authored_buyer",
        "https://zh.jinxxy.com:444/authored_buyer",
        "https://user@zh.jinxxy.com/authored_buyer",
        "https://zh.jinxxy.com/authored_buyer?token=x",
        "https://zh.jinxxy.com/authored_buyer#fragment",
        "/authored_buyer?token=x",
        "/seller/product",
        "/",
    ] {
        assert!(
            protected_profile_route(&format!("<nav><a href='{href}'>Profile</a></nav>")).is_err()
        );
    }
    assert!(protected_profile_route("<a href='/seller'>Profile</a>").is_err());
    assert!(protected_profile_route(
        "<nav><a href='https://zh.jinxxy.com/authored_buyer'>個人資料</a></nav>"
    )
    .is_err());
    assert!(identity(&json!("18446744073709551616")).is_err());
    assert!(identity(&json!("0")).is_err());
    let apex = Url::parse("https://jinxxy.com/").unwrap();
    let localized = Url::parse("https://zh.jinxxy.com/").unwrap();
    assert_eq!(
        native_origin(&Url::parse("https://zh.jinxxy.com/my/inventory?page=2").unwrap()),
        Some(localized.clone())
    );
    assert_eq!(
        inventory_page_at(&localized, 2).as_str(),
        "https://zh.jinxxy.com/my/inventory?page=2"
    );
    // The English page may be served from any Jinxxy host, not only the apex.
    for host in [
        "jinxxy.com",
        "www.jinxxy.com",
        "ja.jinxxy.com",
        "de.jinxxy.com",
        "es.jinxxy.com",
        "fr.jinxxy.com",
        "zh.jinxxy.com",
        "ko.jinxxy.com",
        "other.jinxxy.com",
    ] {
        let page = Url::parse(&format!("https://{host}/my/inventory?page=1")).unwrap();
        assert_eq!(native_origin(&page).unwrap().host_str(), Some(host));
    }
    assert_eq!(native_origin(&inventory_page_at(&apex, 1)).unwrap(), apex);
    for invalid in [
        "https://evil-jinxxy.com/",
        "https://zh.jinxxy.com.example/",
        "https://127.0.0.1/",
        "http://zh.jinxxy.com/",
        "https://user@zh.jinxxy.com/",
        "https://zh.jinxxy.com:444/",
    ] {
        assert!(native_origin(&Url::parse(invalid).unwrap()).is_none());
    }
    use reqwest::cookie::CookieStore;
    let jar = reqwest::cookie::Jar::default();
    jar.add_cookie_str("authored=fixture; Path=/; Secure", &localized);
    assert!(jar.cookies(&inventory_page_at(&apex, 1)).is_none());
    assert!(jar.cookies(&inventory_page_at(&localized, 1)).is_some());
}
#[test]
fn inventory_scope_duplicate_ids_and_incomplete_file_sets_remain_explicit() {
    let row = json!({"id":INVENTORY,"item":{"id":PRODUCT,"name":"Fixture"}});
    let data=buyer_data(&script(&json!({"rehydrate":{"q":{"data":{"inventory_items":{"page":1,"page_count":2,"payload":[row.clone()]}}}}}))).unwrap();
    assert_eq!(
        parse_inventory(&data, 1).unwrap().items[0]
            .product_id
            .as_deref(),
        Some(PRODUCT)
    );
    assert!(parse_inventory(&data, 2).is_err());
    assert_eq!(
        inventory_page_url(2).as_str(),
        "https://jinxxy.com/my/inventory?page=2"
    );
    assert!(parse_inventory(
        &[json!({"inventory_items":{"page":2,"page_count":1,"payload":[]}})],
        2
    )
    .is_err());
    assert!(parse_inventory(
        &[json!({"inventory_items":{"page":1,"page_count":1,"payload":[]}})],
        1
    )
    .unwrap()
    .items
    .is_empty());
    assert!(parse_inventory(
        &[json!({"inventory_items":{"page":1,"page_count":2,"payload":[]}})],
        1
    )
    .is_err());
    let duplicate =
        vec![json!({"inventory_items":{"page":1,"page_count":1,"payload":[row.clone(),row]}})];
    assert!(parse_inventory(&duplicate, 1).is_err());
    let data = vec![json!({"inventory_item":{"id":INVENTORY,"item":{"id":PRODUCT,"files":null}}})];
    assert!(
        !parse_detail(&data, INVENTORY, Some(PRODUCT))
            .unwrap()
            .inventory_complete
    );
    assert!(inventory_file_endpoint("../escape", FILE).is_err());
    assert_eq!(
        inventory_file_endpoint(INVENTORY, FILE).unwrap().host_str(),
        Some("api.jinxxy.com")
    );
}
#[test]
fn orders_with_null_inventory_do_not_turn_order_totals_into_product_paid_price() {
    let order = json!({"order":{"id":INVENTORY,"payment_status":"PAID","paid_at":"2026-10-10T01:02:03Z","order_total":12345,"currency_code":"USD","inventory_items":null,"order_items":[{"id":FILE,"target_type":"PLATFORM_FEE","gross_price":12345,"description":"AUTHORED_PRIVATE"}]}});
    let html = format!(
        "<script>self.__next_f.push({});</script>",
        json!([1, format!("a:{order}\n")])
    );
    let payment = parse_order_payment(&html, INVENTORY, INVENTORY, PRODUCT).unwrap();
    assert!(payment.paid_price.is_none());
    assert!(payment.paid_currency.is_none());
    assert!(payment.paid_at.is_none());
    let mut selected = Vec::new();
    needed_orders(&order, &mut selected);
    let serialized = serde_json::to_string(&selected).unwrap();
    assert!(!serialized.contains("order_total"));
    assert!(!serialized.contains("AUTHORED_PRIVATE"));
    assert!(!serialized.contains("paidPrice"));
}
#[test]
fn head_resolution_keeps_the_signed_location_on_any_public_https_host() {
    let endpoint = inventory_file_endpoint(INVENTORY, FILE).unwrap();
    let found = reqwest::StatusCode::FOUND;
    for location in [
            "https://jinxxy-products.62bb56dad0a3633d04ee066263e8507d.r2.cloudflarestorage.com/authored/file.zip?X-Amz-Signature=authored-only",
            "https://other.r2.cloudflarestorage.com/file.zip?X-Amz-Signature=authored-only",
            "https://authored-cdn.example/file.zip?signature=authored-only",
        ] {
            let url = resolved_location(&endpoint, found, Some(location)).unwrap();
            assert_eq!(url.as_str(), location);
        }
    for status in [301, 303, 307, 308] {
        let status = reqwest::StatusCode::from_u16(status).unwrap();
        assert!(resolved_location(
            &endpoint,
            status,
            Some("https://authored-cdn.example/file.zip")
        )
        .is_ok());
    }
    assert!(resolved_location(
        &endpoint,
        reqwest::StatusCode::FORBIDDEN,
        Some("https://authored-cdn.example/file.zip")
    )
    .is_err());
    assert!(resolved_location(&endpoint, reqwest::StatusCode::OK, None).is_err());
    assert!(resolved_location(&endpoint, found, None).is_err());
    for location in [
        "http://authored-cdn.example/file.zip",
        "https://user@authored-cdn.example/file.zip",
        "https://127.0.0.1/file.zip",
        "https://10.0.0.1/file.zip",
        "https://localhost/file.zip",
    ] {
        assert_eq!(
            resolved_location(&endpoint, found, Some(location))
                .unwrap_err()
                .to_string(),
            "UNSAFE_DOWNLOAD_URL",
            "{location}"
        );
    }
    // The store's sign-in page is a sign-in answer, never a file.
    for location in [
        "https://jinxxy.com/login?redirect=%2Fmy%2Finventory",
        "/login",
    ] {
        assert_eq!(
            resolved_location(&endpoint, found, Some(location))
                .unwrap_err()
                .to_string(),
            "AUTH_REQUIRED",
            "{location}"
        );
    }
}
#[test]
fn file_endpoint_alone_gets_the_inventory_pages_cookies() {
    use reqwest::cookie::CookieStore;
    let jar = reqwest::cookie::Jar::default();
    let site = Url::parse("https://jinxxy.com/").unwrap();
    jar.add_cookie_str("authored_host=HOST_VALUE; Path=/; Secure", &site);
    jar.add_cookie_str(
        "authored_domain=DOMAIN_VALUE; Domain=.jinxxy.com; Path=/; Secure",
        &site,
    );
    let page = Url::parse(&format!("https://jinxxy.com/my/inventory/{INVENTORY}")).unwrap();
    let endpoint = inventory_file_endpoint(INVENTORY, FILE).unwrap();
    let header = |jar: &dyn CookieStore, url: &str| {
        jar.cookies(&Url::parse(url).unwrap())
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    // The shared jar keeps the host's own cookie away from the API host.
    assert!(!header(&jar, endpoint.as_str()).contains("authored_host="));
    let scoped = endpoint_jar(&jar, &page, &endpoint);
    let sent = header(&*scoped, endpoint.as_str());
    assert!(sent.contains("authored_host=HOST_VALUE"), "{sent}");
    assert!(sent.contains("authored_domain=DOMAIN_VALUE"), "{sent}");
    let plain_http = endpoint.as_str().replacen("https://", "http://", 1);
    for other in [
        "https://jinxxy.com/my/inventory",
        "https://cdn.jinxxy.com/file.zip",
        "https://authored-cdn.example/file.zip",
        plain_http.as_str(),
    ] {
        assert!(header(&*scoped, other).is_empty(), "{other}");
    }
}
#[test]
fn one_files_sign_in_answer_or_expired_link_is_not_account_expiry() {
    let path =
        std::env::temp_dir().join(format!("nna-jinxxy-file-answer-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let account = "d0140000-0000-4000-8000-000000000001";
    service.inner.downloads.lock().unwrap().insert(
        account.into(),
        DownloadSession {
            client: reqwest::Client::new(),
            jar: Arc::new(reqwest::cookie::Jar::default()),
            origin: Url::parse("https://jinxxy.com/").unwrap(),
            identity: "authored-binding".into(),
            verified_at: Instant::now(),
            manifests: BTreeMap::new(),
        },
    );
    let kept = service.file_answer(account, anyhow::anyhow!("NETWORK_UNAVAILABLE"));
    assert_eq!(kept.to_string(), "NETWORK_UNAVAILABLE");
    assert!(service
        .inner
        .downloads
        .lock()
        .unwrap()
        .contains_key(account));
    let mapped = service.file_answer(account, anyhow::anyhow!("AUTH_REQUIRED"));
    assert_eq!(mapped.to_string(), "DOWNLOAD_RESOLVER_FAILED");
    // The next ticket verifies the account's inventory again, the account-wide evidence.
    assert!(!service
        .inner
        .downloads
        .lock()
        .unwrap()
        .contains_key(account));
    use reqwest::ResponseBuilderExt;
    let response = |url: &str, status: u16| {
        reqwest::Response::from(
            tauri::http::Response::builder()
                .status(status)
                .url(Url::parse(url).unwrap())
                .body(Vec::<u8>::new())
                .unwrap(),
        )
    };
    let signed = "https://authored-cdn.example/authored.zip?signature=authored-only";
    assert!(validate_download_response(&response(signed, 200), None).is_ok());
    for status in [401, 403] {
        assert_eq!(
            validate_download_response(&response(signed, status), None)
                .unwrap_err()
                .to_string(),
            "SIGNED_LINK_EXPIRED"
        );
    }
    for url in [
        "https://jinxxy.com/login?redirect=%2Fmy%2Finventory",
        "https://127.0.0.1/authored.zip",
        "http://authored-cdn.example/authored.zip",
    ] {
        assert_eq!(
            validate_download_response(&response(url, 200), None)
                .unwrap_err()
                .to_string(),
            "UNSAFE_DOWNLOAD_URL"
        );
    }
    // The compiled flow stays bound to the origin its token was issued for.
    let issued = Url::parse("https://download.example/").unwrap();
    assert!(validate_download_response(
        &response("https://download.example/compiled", 200),
        Some(&issued)
    )
    .is_ok());
    assert!(validate_download_response(&response(signed, 200), Some(&issued)).is_err());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn flight_receipt_uses_unique_bound_product_gross_and_acquisition_precedence() {
    let order_id = "444444444444444444";
    let mut receipt = json!({"order":{"id":order_id,"payment_status":"PAID","paid_at":"2026-01-03T00:00:00Z","currency_code":"USD","order_total":3030,"payout_total":2450,"email":"AUTHORED_PRIVATE","order_items":[
        {"target_type":"PLATFORM_FEE","gross_price":350},
        {"target_type":"DIGITAL_PRODUCT","gross_price":3000,"net_price":2650,"price":3000,"inventory_item":{"id":INVENTORY,"item":{"id":PRODUCT},"license":"AUTHORED_PRIVATE"}}
    ]}});
    let html = |value: &Value| {
        let flight = format!("a:{}\n", json!(["$", "fixture", value]));
        let split = flight.len() / 2;
        format!(
            "<script>self.__next_f.push({});</script><script>self.__next_f.push({})</script>",
            json!([1, &flight[..split]]),
            json!([1, &flight[split..]])
        )
    };
    let payment = parse_order_payment(&html(&receipt), order_id, INVENTORY, PRODUCT).unwrap();
    assert_eq!(payment.paid_price, Some(30.0));
    assert_eq!(payment.paid_currency.as_deref(), Some("USD"));
    let mut selected = Vec::new();
    for record in flight_records(&html(&receipt)).unwrap() {
        needed_orders(&record, &mut selected);
    }
    let projected = serde_json::to_string(&selected).unwrap();
    for forbidden in [
        "AUTHORED_PRIVATE",
        "net_price",
        "payout_total",
        "order_total",
        "email",
        "license",
    ] {
        assert!(!projected.contains(forbidden));
    }
    assert!(
        parse_order_payment(&html(&receipt), "555555555555555555", INVENTORY, PRODUCT)
            .unwrap()
            .paid_price
            .is_none()
    );
    assert!(
        parse_order_payment(&html(&receipt), order_id, FILE, PRODUCT)
            .unwrap()
            .paid_price
            .is_none()
    );
    let duplicate = receipt["order"]["order_items"][1].clone();
    receipt["order"]["order_items"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(
        parse_order_payment(&html(&receipt), order_id, INVENTORY, PRODUCT)
            .unwrap()
            .paid_price
            .is_none()
    );
    receipt["order"]["order_items"]
        .as_array_mut()
        .unwrap()
        .pop();
    receipt["order"]["order_items"][1]["gross_price"] = json!(0);
    assert_eq!(
        parse_order_payment(&html(&receipt), order_id, INVENTORY, PRODUCT)
            .unwrap()
            .paid_price,
        Some(0.0)
    );
    receipt["order"]["currency_code"] = json!("JPY");
    assert!(
        parse_order_payment(&html(&receipt), order_id, INVENTORY, PRODUCT)
            .unwrap()
            .paid_price
            .is_none()
    );
    receipt["order"]["payment_status"] = json!("REFUNDED");
    assert!(
        parse_order_payment(&html(&receipt), order_id, INVENTORY, PRODUCT)
            .unwrap()
            .paid_at
            .is_none()
    );
    assert!(flight_records("<script>self.__next_f.push(runCode())</script>").is_err());
    let page = parse_inventory(&[json!({"inventory_items":{"page":1,"page_count":1,"payload":[{"id":INVENTORY,"created_at":"2026-01-02T00:00:00Z","item":{"id":PRODUCT,"created_at":"2020-01-01T00:00:00Z"}}]}})],1).unwrap();
    let mut detail = parse_detail(&[json!({"inventory_item":{"id":INVENTORY,"imported_purchased_at":"2026-01-01T00:00:00Z","item":{"id":PRODUCT,"files":[]}}})],INVENTORY,Some(PRODUCT)).unwrap();
    assert_eq!(
        acquisition_date(&detail, &page.items[0], Some(&payment)),
        date(&json!("2026-01-01T00:00:00Z"))
    );
    detail.imported_purchased_at = None;
    assert_eq!(
        acquisition_date(&detail, &page.items[0], Some(&payment)),
        date(&json!("2026-01-02T00:00:00Z"))
    );
    let mut entry = page.items.into_iter().next().unwrap();
    entry.created_at = None;
    assert_eq!(
        acquisition_date(&detail, &entry, Some(&payment)),
        payment.paid_at
    );
}
#[test]
fn download_cache_expires_and_rejects_changed_identity_or_file_revision() {
    let now = Instant::now();
    let record = json!({"store":"Jinxxy","status":"connected","state":"connected","identityVerified":true,"identityHash":"authored-binding"});
    let session = DownloadSession {
        client: reqwest::Client::new(),
        jar: Arc::new(reqwest::cookie::Jar::default()),
        origin: Url::parse("https://zh.jinxxy.com/").unwrap(),
        identity: "authored-binding".into(),
        verified_at: now,
        manifests: BTreeMap::new(),
    };
    assert!(session.valid(&record, now + Duration::from_secs(119)));
    assert_eq!(session.snapshot().origin, session.origin);
    assert!(!session.valid(&record, now + Duration::from_secs(120)));
    for (key, value) in [
        ("identityHash", json!("other-binding")),
        ("identityVerified", json!(false)),
        ("disconnected", json!(true)),
        ("status", json!("disconnected")),
        ("state", json!("disconnected")),
    ] {
        let mut changed = record.clone();
        changed[key] = value;
        assert!(!session.valid(&changed, now));
    }
    let observed = FileObservation {
        id: FILE.into(),
        name: "authored.zip".into(),
        size_bytes: Some(20),
        updated_at: date(&json!("2026-10-10T01:02:03Z")),
        compiled: false,
        compiled_available: false,
    };
    let file = json!({"sourceFileId":FILE,"revision":file_revision(&observed)});
    let mut moved = observed.clone();
    moved.name = "Other/authored.zip".into();
    assert_ne!(file_revision(&moved), file_revision(&observed));
    let manifest = DownloadManifest {
        product: PRODUCT.into(),
        observed_at: now,
        files: vec![observed],
    };
    assert!(manifest.valid(PRODUCT, &file, now + Duration::from_secs(59)));
    assert!(!manifest.valid(PRODUCT, &file, now + Duration::from_secs(60)));
    assert!(!manifest.valid(INVENTORY, &file, now));
    let mut changed = file.clone();
    changed["revision"] = json!("same-file-new-revision");
    assert!(!manifest.valid(PRODUCT, &changed, now));
    changed = file;
    changed["sourceFileId"] = json!(INVENTORY);
    assert!(!manifest.valid(PRODUCT, &changed, now));
}
#[test]
fn flat_file_paths_preserve_siblings_subfolders_and_compiled_incompleteness() {
    let path = std::env::temp_dir().join(format!(
        "nna-authored-jinxxy-folders-{}",
        uuid::Uuid::new_v4()
    ));
    let catalogue = Catalogue::open(&path).unwrap();
    let account = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let row = json!({"inventory_item":{"id":INVENTORY,"item":{"id":PRODUCT,"files":[
        {"id":FILE,"file_name":"Pack/avatar.unitypackage","size":10},
        {"id":"444444444444444444","file_name":"Pack/Textures/body.png","size":20},
        {"id":"555555555555555555","file_name":"Other/Textures/body.png","size":30}
    ]}}});
    let detail = parse_detail(&[row.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    assert!(detail.inventory_complete);
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &detail, None, None).unwrap();
    batch.commit().unwrap();
    let files = catalogue.list("remote_file").unwrap();
    assert_eq!(files.len(), 3);
    let paths: BTreeSet<_> = files
        .iter()
        .map(|file| file["sourceRelativePath"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        BTreeSet::from([
            "Pack/avatar.unitypackage",
            "Pack/Textures/body.png",
            "Other/Textures/body.png"
        ])
    );
    assert_eq!(
        files
            .iter()
            .filter(|file| file["name"] == "body.png")
            .count(),
        2
    );
    assert!(files
        .iter()
        .all(|file| file["accountId"] == account && file["inventoryId"] == INVENTORY));
    for invalid in [
        "../secret",
        "/absolute",
        "A/../secret",
        "A//leaf",
        "C:/secret",
        "A\\leaf",
    ] {
        assert!(source_file_name(invalid).is_err());
    }
    let mut compiled = row;
    compiled["inventory_item"]["item"]["files"][0]["extension"] = json!("3b");
    let detail = parse_detail(&[compiled.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    assert!(!detail.inventory_complete);
    assert_eq!(detail.files.len(), 3);
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &detail, None, None).unwrap();
    batch.commit().unwrap();
    let file = catalogue
        .get(
            "remote_file",
            &format!("jinxxy:{account}:{INVENTORY}:{FILE}"),
        )
        .unwrap()
        .unwrap();
    assert_eq!(file["deliveryKind"], "jinxxyCompiled3b");
    assert!(file["sizeBytes"].is_null());
    assert_eq!(file["sourceSizeBytes"], 10);
    assert_eq!(
        catalogue
            .get("entitlement", &format!("jinxxy:{account}:{INVENTORY}"))
            .unwrap()
            .unwrap()["manifestComplete"],
        false
    );
    assert!(compiled_request(INVENTORY, &detail.files[0]).is_err());
    compiled["inventory_item"]["item"]["files"][0]["file_meta"] = json!({
        "meta_type":"t3b_meta", "asset_bundles":{
            "windows":{"performance":"good","unrelated":"AUTHORED_PRIVATE"},
            "android":{"performance":"medium"}
        }
    });
    let data = buyer_data(&script(&json!({"events":[{"value":{"data":compiled}}]}))).unwrap();
    assert!(!serde_json::to_string(&data)
        .unwrap()
        .contains("AUTHORED_PRIVATE"));
    let detail = parse_detail(&data, INVENTORY, Some(PRODUCT)).unwrap();
    assert!(detail.inventory_complete);
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &detail, None, None).unwrap();
    batch.commit().unwrap();
    assert_eq!(
        catalogue
            .get("entitlement", &format!("jinxxy:{account}:{INVENTORY}"))
            .unwrap()
            .unwrap()["manifestComplete"],
        true
    );
    assert!(catalogue
        .get(
            "remote_file",
            &format!("jinxxy:{account}:{INVENTORY}:{FILE}")
        )
        .unwrap()
        .unwrap()["sizeBytes"]
        .is_null());
    let request = compiled_request(INVENTORY, &detail.files[0]).unwrap();
    // Both platform entries share one original ID, so it is requested exactly once.
    assert_eq!(
        request["variables"],
        json!({"inventory_id":INVENTORY,"file_ids":[FILE]})
    );
    assert!(compiled_request(INVENTORY, &detail.files[1]).is_err());
    assert!(compiled_request("not-an-inventory-id", &detail.files[0]).is_err());
    let response = json!({"data":{"inventory_item":{
        "downloadT3bFile":{"url":"https://download.example/compiled?cap=authored","token":"AUTHORED_EPHEMERAL_TOKEN"}
    }}});
    let capability = compiled_capability(&response).unwrap();
    assert!(capability.authorization.is_sensitive());
    assert_eq!(
        capability.url.origin().ascii_serialization(),
        "https://download.example"
    );
    let mut invalid = response.clone();
    invalid["data"]["inventory_item"] = Value::Null;
    assert!(compiled_capability(&invalid).is_err());
    invalid = response.clone();
    invalid["data"]["inventory_item"]["downloadT3bFile"]["token"] = json!("invalid\r\nheader");
    assert!(compiled_capability(&invalid).is_err());
    invalid = response.clone();
    invalid["errors"] = json!([{"message":"AUTHORED_PRIVATE"}]);
    assert!(compiled_capability(&invalid).is_err());
    for destination in [
        "http://download.example/file",
        "https://download.example:444/file",
        "https://user@download.example/file",
        "https://download.example/file#fragment",
        "https://127.0.0.1/file",
        "https://10.0.0.1/file",
        "https://169.254.169.254/file",
        "https://[::1]/file",
        "https://[fc00::1]/file",
        "https://host.local/file",
    ] {
        invalid = response.clone();
        invalid["data"]["inventory_item"]["downloadT3bFile"]["url"] = json!(destination);
        assert!(compiled_capability(&invalid).is_err());
    }
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "100.64.0.1",
        "192.168.1.1",
        "::1",
        "fe80::1",
        "::ffff:127.0.0.1",
    ] {
        assert!(!super::super::http::public_ip(address.parse().unwrap()));
    }
    assert!(super::super::http::public_ip("8.8.8.8".parse().unwrap()));
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn native_jinxxy_records_bind_account_inventory_and_preserve_user_overlay_without_secrets() {
    let path = std::env::temp_dir().join(format!("nna-authored-jinxxy-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let product_id = format!("jinxxy:{PRODUCT}");
    let mut row = json!({"inventory_item":{"id":INVENTORY,"license":"AUTHORED_SECRET","item":{"id":PRODUCT,"name":"Authored source title","url":"abc12","user":{"username":"authored-maker"},"base_price":3000,"currency_code":"USD","files":[{"id":FILE,"file_name":"source.psd","size":12,"updated_at":"2026-10-10T00:00:00Z"}],"cover":{"url":"https://jinxxy-c.jinxxy-cdn.com/authored.png"}}}});
    let detail = parse_detail(&[row.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    assert_eq!(
        detail.source_url.as_deref(),
        Some("https://jinxxy.com/authored-maker/abc12")
    );
    assert_eq!(detail.current_price, Some(30.0));
    for segment in [
        "",
        "..",
        "../else",
        "https://example.test",
        "a?token=secret",
        "a%2Fb",
    ] {
        assert!(
            product_url(&json!({"user":{"username":"authored-maker"},"url":segment})).is_none()
        );
    }
    let overlay = json!({"id":product_id,"title":"Authored user title","note":"Authored note","category":"Hair","tags":["Authored manual tag"],"rejectedTags":["rejected"]});
    catalogue
        .put("overlay", &product_id, overlay.clone())
        .unwrap();
    for account in [
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    ] {
        let mut batch = crate::booth::RecordBatch::new(&catalogue);
        stage_detail(&mut batch, account, &detail, None, None).unwrap();
        batch.commit().unwrap();
    }
    // Prior same-product-bound public presentation is not supplied by the
    // buyer inventory query. An absent later public response must retain it.
    let mut presentation = catalogue.get("product", &product_id).unwrap().unwrap();
    presentation["sourceTags"] = json!(["Authored source tag", "rejected"]);
    presentation["sourceDescription"] = json!("Authored public description");
    catalogue.put("product", &product_id, presentation).unwrap();
    let assert_retained = |catalogue: &Catalogue, price: f64| {
        let source = catalogue.get("product", &product_id).unwrap().unwrap();
        assert_eq!(source["price"], price);
        assert_eq!(source["priceCurrency"], "USD");
        assert_eq!(
            source["sourceTags"],
            json!(["Authored source tag", "rejected"])
        );
        assert_eq!(source["sourceDescription"], "Authored public description");
        assert_eq!(
            catalogue.get("overlay", &product_id).unwrap().unwrap(),
            overlay
        );
        let projected = catalogue
            .command("product_details", json!({"id":product_id}))
            .unwrap();
        assert_eq!(projected["price"], price);
        assert_eq!(projected["title"], "Authored user title");
        assert_eq!(projected["note"], "Authored note");
        assert_eq!(projected["category"], "Hair");
        assert_eq!(
            projected["accountIds"],
            json!([
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
            ])
        );
        assert!(projected["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "Authored manual tag"));
        assert!(!projected["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "rejected"));
        assert_eq!(projected["rejectedTags"], json!(["rejected"]));
    };
    assert_retained(&catalogue, 30.0);
    let files = catalogue.list("remote_file").unwrap();
    assert_eq!(files.len(), 2);
    assert_ne!(files[0]["id"], files[1]["id"]);
    assert!(files.iter().all(|file| file["productId"] == product_id
        && file["sourceFileId"] == FILE
        && file.get("sourceUrl").is_none()));
    assert!(catalogue
        .list("acquisition")
        .unwrap()
        .iter()
        .all(|row| row["acquiredAt"].is_null() && row["paidPrice"].is_null()));
    assert_eq!(
        catalogue.get("overlay", &product_id).unwrap().unwrap()["title"],
        "Authored user title"
    );
    assert_eq!(
        catalogue.get("product", &product_id).unwrap().unwrap()["previewUrl"],
        "https://jinxxy-c.jinxxy-cdn.com/authored.png"
    );
    assert!(!serde_json::to_string(&catalogue.list("product").unwrap())
        .unwrap()
        .contains("AUTHORED_SECRET"));
    let projected = catalogue
        .command("product_details", json!({"id":product_id}))
        .unwrap();
    assert_eq!(projected["price"], 30.0);
    assert_eq!(projected["priceCurrency"], "USD");
    assert_eq!(
        projected["sourceUrl"],
        "https://jinxxy.com/authored-maker/abc12"
    );
    assert_eq!(
        catalogue.get("product", &product_id).unwrap().unwrap()["priceBasis"],
        "jinxxyBasePrice"
    );
    row["inventory_item"]["item"]["base_price"] = json!(0);
    let free = parse_detail(&[row.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(
        &mut batch,
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        &free,
        None,
        None,
    )
    .unwrap();
    batch.commit().unwrap();
    assert_eq!(
        catalogue
            .command("product_details", json!({"id":product_id}))
            .unwrap()["price"],
        0.0
    );
    row["inventory_item"]["item"]["currency_code"] = json!("JPY");
    let account = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let unknown_listing_price = parse_detail(&[row.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    assert!(unknown_listing_price.current_price.is_none());
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &unknown_listing_price, None, None).unwrap();
    batch.commit().unwrap();
    let unknown_price_product = catalogue.get("product", &product_id).unwrap().unwrap();
    assert_eq!(unknown_price_product["price"], 0.0);
    assert_eq!(unknown_price_product["priceCurrency"], "USD");
    let entitlement = format!("jinxxy:{account}:{INVENTORY}");
    let original_file = format!("jinxxy:{account}:{INVENTORY}:{FILE}");
    let new_source = "7777777777777777777";
    let new_file = format!("jinxxy:{account}:{INVENTORY}:{new_source}");
    let acquired = date(&json!("2026-01-02T00:00:00Z"));
    let known_payment = OrderPayment {
        paid_price: Some(12.5),
        paid_currency: Some("USD".into()),
        paid_at: acquired,
    };
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &free, acquired, Some(&known_payment)).unwrap();
    batch.commit().unwrap();
    // One unnamed offered file makes this refresh partial. Its known membership
    // and prior financial observations must survive both absent and unbound payment.
    row["inventory_item"]["item"]["files"] = json!([
        {"id":FILE,"size":12},
        {"id":new_source,"file_name":"additional.txt","size":5}
    ]);
    let partial = parse_detail(&[row.clone()], INVENTORY, Some(PRODUCT)).unwrap();
    assert!(!partial.inventory_complete);
    let unknown_payment = OrderPayment::default();
    for payment in [None, Some(&unknown_payment)] {
        let mut batch = crate::booth::RecordBatch::new(&catalogue);
        stage_detail(&mut batch, account, &partial, None, payment).unwrap();
        batch.commit().unwrap();
        let source = catalogue.get("product", &product_id).unwrap().unwrap();
        assert_eq!(source["price"], 0.0);
        assert_eq!(source["priceCurrency"], "USD");
        assert_eq!(source["priceBasis"], "jinxxyBasePrice");
        assert_retained(&catalogue, 0.0);
        let purchase = catalogue.get("acquisition", &entitlement).unwrap().unwrap();
        assert_eq!(purchase["paidPrice"], 12.5);
        assert_eq!(purchase["paidCurrency"], "USD");
        assert_eq!(purchase["paidPriceBasis"], "productLineTotal");
        assert_eq!(purchase["acquiredAt"], acquired.unwrap().to_rfc3339());
        let grant = catalogue.get("entitlement", &entitlement).unwrap().unwrap();
        assert_eq!(grant["manifestComplete"], false);
        let required: BTreeSet<_> = grant["fileIds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            required,
            BTreeSet::from([original_file.as_str(), new_file.as_str()])
        );
    }
    drop(catalogue);
    let catalogue = Catalogue::open(&path).unwrap();
    assert_retained(&catalogue, 0.0);
    let retained_partial = catalogue.get("entitlement", &entitlement).unwrap().unwrap();
    assert_eq!(retained_partial["manifestComplete"], false);
    assert_eq!(retained_partial["fileIds"].as_array().unwrap().len(), 2);
    assert_eq!(
        catalogue.get("acquisition", &entitlement).unwrap().unwrap()["paidPrice"],
        12.5
    );
    // A complete observation may replace membership and a genuine zero payment.
    row["inventory_item"]["item"]["files"] =
        json!([{"id":new_source,"file_name":"additional.txt","size":5}]);
    row["inventory_item"]["item"]["base_price"] = json!(900);
    row["inventory_item"]["item"]["currency_code"] = json!("USD");
    let complete = parse_detail(&[row], INVENTORY, Some(PRODUCT)).unwrap();
    let zero_payment = OrderPayment {
        paid_price: Some(0.0),
        paid_currency: Some("USD".into()),
        paid_at: None,
    };
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    stage_detail(&mut batch, account, &complete, None, Some(&zero_payment)).unwrap();
    batch.commit().unwrap();
    let grant = catalogue.get("entitlement", &entitlement).unwrap().unwrap();
    assert_eq!(grant["manifestComplete"], true);
    assert_eq!(grant["fileIds"], json!([new_file]));
    assert!(catalogue
        .get("remote_file", &original_file)
        .unwrap()
        .is_some());
    assert_eq!(
        catalogue.get("product", &product_id).unwrap().unwrap()["price"],
        9.0
    );
    assert_eq!(
        catalogue.get("acquisition", &entitlement).unwrap().unwrap()["paidPrice"],
        0.0
    );
    assert_eq!(
        catalogue.get("overlay", &product_id).unwrap().unwrap()["title"],
        "Authored user title"
    );
    assert_retained(&catalogue, 9.0);
    let coverage_id = format!("jinxxy:{account}:inventory");
    let prior_success = "2026-01-02T00:00:00+00:00";
    catalogue
            .put(
                "source_coverage",
                &coverage_id,
                json!({
                    "id":coverage_id,"provider":"jinxxy","accountId":account,"source":"library",
                    "status":"complete","coverageComplete":true,"productCount":1,"completedAt":prior_success,"updatedAt":prior_success
                }),
            )
            .unwrap();
    let service = JinxxyService::new(catalogue.clone()).unwrap();
    let mut job = json!({"type":"jinxxySync","accountId":account,"completedProducts":3});
    service.coverage(&job, "running", None).unwrap();
    let coverage = || {
        catalogue
            .get("source_coverage", &coverage_id)
            .unwrap()
            .unwrap()
    };
    let running = coverage();
    assert_eq!(running["store"], "Jinxxy");
    assert_eq!(running["status"], "running");
    assert_eq!(running["coverageComplete"], false);
    assert_eq!(running["lastSuccessfulAt"], prior_success);
    assert!(running.get("state").is_none() && running.get("complete").is_none());
    for (state, code, expected) in [
        ("needsAction", "AUTH_REQUIRED", "auth-required"),
        ("failed", "JINXXY_SOURCE_UNSUPPORTED", "unsupported"),
        ("failed", "NETWORK_UNAVAILABLE", "failed"),
        ("failed", "VERIFICATION_PAGE_LIMIT", "incomplete"),
        ("cancelled", "CANCELLED", "cancelled"),
    ] {
        service.coverage(&job, state, Some(code)).unwrap();
        let terminal = coverage();
        assert_eq!(terminal["status"], expected);
        assert_eq!(terminal["coverageComplete"], false);
        assert_eq!(terminal["errorCode"], code);
        assert_eq!(terminal["lastSuccessfulAt"], prior_success);
        assert!(date(&terminal["completedAt"]).is_some());
    }
    service.coverage(&job, "complete", None).unwrap();
    let complete_coverage = coverage();
    assert_eq!(complete_coverage["status"], "complete");
    assert_eq!(complete_coverage["coverageComplete"], true);
    assert_eq!(complete_coverage["productCount"], 3);
    assert_eq!(
        complete_coverage["lastSuccessfulAt"],
        complete_coverage["completedAt"]
    );
    assert!(complete_coverage["errorCode"].is_null());
    job["type"] = json!("jinxxyUpdate");
    service
        .coverage(&job, "failed", Some("NETWORK_UNAVAILABLE"))
        .unwrap();
    assert_eq!(coverage(), complete_coverage);
    // Multiple grants are one publication unit, including on an incomplete
    // second grant. Prepared rows never change the prior visible snapshot.
    let prior_product = catalogue.get("product", &product_id).unwrap();
    let selected = json!({"type":"jinxxyUpdate","productIds":[product_id]});
    assert_eq!(
        missing_selected(&selected, &BTreeSet::new()).unwrap(),
        BTreeSet::from([PRODUCT.to_owned()])
    );
    assert!(
        missing_selected(&selected, &BTreeSet::from([PRODUCT.to_owned()]))
            .unwrap()
            .is_empty()
    );
    let mixed_selected =
        json!({"type":"jinxxyUpdate","productIds":[product_id,"jinxxy:1234567890123456791"]});
    assert_eq!(
        missing_selected(&mixed_selected, &BTreeSet::from([PRODUCT.to_owned()])).unwrap(),
        BTreeSet::from(["1234567890123456791".to_owned()])
    );
    for malformed in [json!([]), json!(null), json!(["booth:1"]), json!([1])] {
        let job = json!({"type":"jinxxyUpdate","productIds":malformed});
        assert!(missing_selected(&job, &BTreeSet::new()).is_err());
    }
    let second_inventory = "1234567890123456790";
    let first_row = json!({"inventory_item":{"id":INVENTORY,"item":{"id":PRODUCT,"name":"New grouped title","files":[{"id":FILE,"file_name":"one.txt","size":1}]}}});
    let second_row =
        json!({"inventory_item":{"id":second_inventory,"item":{"id":PRODUCT,"files":null}}});
    let mut group = vec![
        (
            parse_detail(&[first_row], INVENTORY, Some(PRODUCT)).unwrap(),
            None,
            None,
        ),
        (
            parse_detail(&[second_row], second_inventory, Some(PRODUCT)).unwrap(),
            None,
            None,
        ),
    ];
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    assert!(stage_complete_group(&mut batch, account, &group).is_err());
    assert_eq!(batch.len(), 0);
    assert_eq!(
        catalogue.get("product", &product_id).unwrap(),
        prior_product
    );
    group[1].0.inventory_complete = true;
    group[1].0.files = group[0].0.files.clone();
    stage_observed_group(&mut batch, account, &group).unwrap();
    assert_eq!(
        catalogue.get("product", &product_id).unwrap(),
        prior_product
    );
    batch.commit().unwrap();
    assert_retained(&catalogue, 9.0);
    for inventory in [INVENTORY, second_inventory] {
        assert_eq!(
            catalogue
                .get("entitlement", &format!("jinxxy:{account}:{inventory}"))
                .unwrap()
                .unwrap()["manifestComplete"],
            false
        );
    }
    job["id"] = json!("jinxxy:authored-membership");
    job["state"] = json!("running");
    catalogue
        .put("job", "jinxxy:authored-membership", job.clone())
        .unwrap();
    service
        .certify_product_membership(
            &job,
            account,
            &product_id,
            &BTreeSet::from([INVENTORY.to_owned(), second_inventory.to_owned()]),
        )
        .unwrap();
    assert!(catalogue
        .related("entitlement", &product_id)
        .unwrap()
        .iter()
        .all(|grant| grant["manifestComplete"] == true));
    assert_retained(&catalogue, 9.0);
    // A complete, bound group publishes without optional receipt data;
    // the earlier proven zero payment survives, and the new grant stays unknown.
    assert_eq!(
        catalogue.get("acquisition", &entitlement).unwrap().unwrap()["paidPrice"],
        0.0
    );
    assert!(catalogue
        .get(
            "acquisition",
            &format!("jinxxy:{account}:{second_inventory}")
        )
        .unwrap()
        .unwrap()["paidPrice"]
        .is_null());
    catalogue
        .put(
            "account",
            account,
            json!({"id":account,"store":"Jinxxy","lastSyncAt":prior_success}),
        )
        .unwrap();
    job["id"] = json!("jinxxy:authored-terminal");
    job["type"] = json!("jinxxySync");
    job["state"] = json!("running");
    catalogue
        .put("job", "jinxxy:authored-terminal", job.clone())
        .unwrap();
    service.finish_job(&mut job, "completed", None).unwrap();
    assert_eq!(job["progress"]["stage"], "completed");
    assert!(date(&job["completedAt"]).is_some());
    let successful_at = catalogue.get("account", account).unwrap().unwrap()["lastSyncAt"].clone();
    assert_eq!(successful_at, job["completedAt"]);
    let mut cancelled = job.clone();
    cancelled["cancelRequested"] = json!(true);
    catalogue
        .put("job", "jinxxy:authored-terminal", cancelled)
        .unwrap();
    service.finish_job(&mut job, "completed", None).unwrap();
    assert_eq!(job["state"], "cancelled");
    assert_eq!(job["progress"]["stage"], "cancelled");
    assert_eq!(coverage()["coverageComplete"], false);
    let account_row = catalogue.get("account", account).unwrap().unwrap();
    assert_eq!(account_row["lastSyncAt"], successful_at);
    assert_eq!(account_row["syncState"], "incomplete");
    assert_eq!(account_row["lastSyncErrorCode"], "CANCELLED");
    drop(service);
    drop(catalogue);
    let reopened = Catalogue::open(&path).unwrap();
    assert_retained(&reopened, 9.0);
    let retained_purchase = reopened.get("acquisition", &entitlement).unwrap().unwrap();
    assert_eq!(retained_purchase["accountId"], account);
    assert_eq!(retained_purchase["paidPrice"], 0.0);
    assert_eq!(retained_purchase["paidCurrency"], "USD");
    assert_eq!(retained_purchase["paidPriceBasis"], "productLineTotal");
    assert_eq!(
        retained_purchase["acquiredAt"],
        acquired.unwrap().to_rfc3339()
    );
    assert_eq!(
        reopened.related("entitlement", &product_id).unwrap().len(),
        3
    );
    assert!(reopened
        .related("entitlement", &product_id)
        .unwrap()
        .iter()
        .all(|grant| grant["manifestComplete"] == true));
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}
