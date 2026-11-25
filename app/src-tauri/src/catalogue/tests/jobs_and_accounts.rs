//! Accounts, their removal and login cleanup, job diagnostics and Activity.
use super::*;

#[test]
fn batch_import_status_snapshot_preserves_completed_members_and_updates_after_job_changes() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let mut records = (0..10000)
        .map(|n| {
            let id = format!("authored-history-{n}");
            (
                "job".into(),
                id.clone(),
                json!({"id":id,"type":"download","state":"completed"}),
            )
        })
        .collect::<Vec<_>>();
    for id in ["one", "two"] {
        records.push((
            "product".into(),
            id.into(),
            json!({"id":id,"title":"Authored import","sourceCategory":{"name":"Tool"}}),
        ));
    }
    let mut job = json!({"id":"authored-import","type":"storage:import_local","state":"running","productIds":["one","two"],"completedProductIds":["one"]});
    records.push(("job".into(), "authored-import".into(), job.clone()));
    c.put_many(records).unwrap();
    let status = |id| c.command("product_details", json!({"id":id})).unwrap()["status"].clone();
    assert_eq!(status("one"), "notDownloaded");
    assert_eq!(status("two"), "processing");
    job["completedProductIds"] = json!(["one", "two"]);
    c.put("job", "authored-import", job).unwrap();
    assert_eq!(status("one"), "notDownloaded");
    assert_eq!(status("two"), "notDownloaded");
}

#[test]
fn job_diagnostics_is_read_only_private_and_does_not_mix_later_account_snapshots() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let job = json!({"id":"PRIVATE_JOB","accountId":"PRIVATE_ACCOUNT","accountName":"PRIVATE_NAME",
        "type":"boothSync","state":"failed","errorCode":"SOURCE_HTTP_FAILED",
        "args":{"note":"PRIVATE_INPUT","url":"https://example.invalid/PRIVATE_ROUTE"},
        "progress":{"stage":"failed","pages":2},"supportDiagnostics":{"version":1,"lastStage":"collecting",
            "sources":{"purchase":{"state":"failed","errorCode":"SOURCE_HTTP_FAILED"}},
            "http":{"receipt":{"requests":1,"lastStatus":502}},"financial":{"receiptPages":1,"rowsWithPaidProof":0}}});
    c.put("job", "PRIVATE_JOB", job.clone()).unwrap();
    c.put("source_coverage", "PRIVATE_ACCOUNT:purchase", json!({"id":"PRIVATE_ACCOUNT:purchase","accountId":"PRIVATE_ACCOUNT","source":"purchase","pages":999,"status":"complete"})).unwrap();
    c.put("checkpoint", "PRIVATE_CHECKPOINT", json!({"id":"PRIVATE_CHECKPOINT","accountId":"PRIVATE_ACCOUNT","source":"purchase","pages":998,"routes":["PRIVATE_ROUTE"]})).unwrap();
    let before = c.change_domains().unwrap();
    let report = c
        .command("job_diagnostics", json!({"jobId":"PRIVATE_JOB"}))
        .unwrap();
    let text = report.as_str().unwrap();
    let value: Value = serde_json::from_str(text).unwrap();
    assert_eq!(value["captureAvailable"], true);
    assert_eq!(
        value["supportDiagnostics"]["http"]["receipt"]["lastStatus"],
        502
    );
    assert!(value.get("sourceCoverage").is_none());
    assert!(value.get("checkpoints").is_none());
    for secret in ["PRIVATE", "999", "998", "https://", "accountId", "cookie"] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert_eq!(c.change_domains().unwrap(), before);
    assert_eq!(c.get("job", "PRIVATE_JOB").unwrap(), Some(job));
    assert_eq!(
        c.command("job_diagnostics", json!({"jobId":"MISSING_PRIVATE_ID"}))
            .unwrap_err()
            .to_string(),
        "JOB_UNAVAILABLE"
    );
}

#[test]
fn account_removal_busy_guard_preserves_records_until_related_jobs_are_terminal() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put("account", "a", json!({"id":"a","identityVerified":true}))
        .unwrap();
    for (kind, product_id) in [
        ("acquisition", "purchase-product"),
        ("entitlement", "entitled-product"),
        ("remote_file", "file-product"),
    ] {
        product(&c, product_id);
        c.put(
            kind,
            kind,
            json!({"id":kind,"productId":product_id,"accountId":"a"}),
        )
        .unwrap();
    }
    for binding in [
        json!({"type":"boothLogin","accountId":"a","state":"queued"}),
        json!({"type":"gumroadSync","accountId":"a","state":"running"}),
        json!({"type":"download","productId":"purchase-product","state":"queued"}),
        json!({"type":"download","productIds":["unrelated","entitled-product"],"state":"running"}),
        json!({"type":"storage:import_local","productIds":["file-product"],"state":"running"}),
    ] {
        let mut job = binding;
        job["id"] = json!("busy");
        c.put("job", "busy", job.clone()).unwrap();
        let revision = c.change_domains().unwrap();
        assert_eq!(
            c.require_account_idle("a").unwrap_err().to_string(),
            "ACCOUNT_BUSY"
        );
        for remove_products in [false, true] {
            assert_eq!(
                c.remove_account("a", remove_products)
                    .unwrap_err()
                    .to_string(),
                "ACCOUNT_BUSY"
            );
            assert!(c.get("account", "a").unwrap().is_some());
            for (kind, product_id) in [
                ("acquisition", "purchase-product"),
                ("entitlement", "entitled-product"),
                ("remote_file", "file-product"),
            ] {
                assert!(c.get("product", product_id).unwrap().is_some());
                assert!(c.get(kind, kind).unwrap().is_some());
            }
            assert_eq!(c.change_domains().unwrap(), revision);
        }
        job["state"] = json!("cancelled");
        c.put("job", "busy", job).unwrap();
        c.require_account_idle("a").unwrap();
    }
    // Other accounts' active work and unrelated products do not block removal.
    c.put(
        "job",
        "other",
        json!({"id":"other","type":"boothSync","accountId":"b","state":"running"}),
    )
    .unwrap();
    c.put("job", "unrelated", json!({"id":"unrelated","type":"download","productId":"unrelated","productIds":["also-unrelated"],"state":"queued"})).unwrap();
    c.require_account_idle("a").unwrap();
    assert_eq!(
        c.remove_account("a", true).unwrap(),
        json!({"accountRemoved":1,"productRemoved":3,"retainedShared":0})
    );
    assert_eq!(c.get("job", "busy").unwrap().unwrap()["state"], "cancelled");
    assert_eq!(c.get("job", "other").unwrap().unwrap()["state"], "running");
}

#[test]
fn account_removal_without_products_preserves_history_overlays_and_downloaded_files() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "p");
    c.put(
        "account",
        "a",
        json!({"id":"a","name":"Authored account","authenticatedAt":"2026-10-11T00:00:00Z"}),
    )
    .unwrap();
    let acquisition = json!({"id":"purchase","productId":"p","accountId":"a","source":"purchase","sourceRecordId":"authored-receipt","acquiredDateText":"2026/10/01","paidPrice":1200,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"});
    let file = json!({"id":"f","productId":"p","accountId":"a","name":"Authored.unitypackage","revision":"r1"});
    let entitlement =
        json!({"id":"e","productId":"p","accountId":"a","fileIds":["f"],"manifestComplete":true});
    let relative = "Authored--p/source/Authored.unitypackage";
    let path = fixture.0.join(relative);
    let bytes = b"independently authored downloaded content";
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    use sha2::{Digest, Sha256};
    let artifact = json!({"id":"artifact","productId":"p","fileId":"f","revision":"r1","rootId":"r","status":"ready","originalAvailable":true,"relativePath":relative,"productDirectory":"Authored--p","sizeBytes":bytes.len(),"hash":format!("{:x}",Sha256::digest(bytes))});
    c.put(
        "root",
        "r",
        json!({"id":"r","path":fixture.0,"status":"online"}),
    )
    .unwrap();
    c.put_many(vec![
        ("acquisition".into(), "purchase".into(), acquisition.clone()),
        ("entitlement".into(), "e".into(), entitlement.clone()),
        ("remote_file".into(), "f".into(), file.clone()),
        ("artifact".into(), "artifact".into(), artifact.clone()),
        ("overlay".into(), "p".into(), json!({"id":"p","note":"Human note","rejectedTags":["blue"]})),
        ("job".into(), "login".into(), json!({"id":"login","accountId":"a","type":"boothLogin","state":"failed","errorCode":"AUTH_REQUIRED"})),
    ]).unwrap();
    let before = dynamic_details(&c, "p");
    assert_eq!(before["status"], "ready");
    assert_eq!(
        c.remove_account("a", false).unwrap(),
        json!({"accountRemoved":1,"productRemoved":0,"retainedShared":0})
    );
    assert!(c.get("account", "a").unwrap().is_none());
    for (kind, id, expected) in [
        ("acquisition", "purchase", acquisition),
        ("entitlement", "e", entitlement),
        ("remote_file", "f", file),
        ("artifact", "artifact", artifact),
    ] {
        assert_eq!(c.get(kind, id).unwrap(), Some(expected));
    }
    let after = dynamic_details(&c, "p");
    for field in [
        "status",
        "downloaded",
        "acquired",
        "paidPrice",
        "paidCurrency",
        "note",
        "rejectedTags",
    ] {
        assert_eq!(after[field], before[field], "{field}");
    }
    assert_eq!(after["provenance"][0]["paidPrice"].as_f64(), Some(1200.0));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(c.get("job", "login").unwrap().unwrap()["state"], "failed");
    assert_eq!(
        c.remove_account("a", true).unwrap(),
        json!({"accountRemoved":0,"productRemoved":0,"retainedShared":0})
    );
    let other = Fixture::new();
    let peer = other.open();
    peer.restore_manifest(c.manifest().unwrap()).unwrap();
    assert_eq!(
        peer.get("acquisition", "purchase").unwrap().unwrap()["paidPrice"].as_f64(),
        Some(1200.0)
    );
    assert_eq!(
        peer.get("overlay", "p").unwrap().unwrap()["note"],
        "Human note"
    );
    assert_eq!(
        peer.get("artifact", "artifact").unwrap().unwrap()["relativePath"],
        relative
    );
}

#[test]
fn account_removal_deletes_only_exclusive_products_and_own_source_facts() {
    let fixture = Fixture::new();
    let c = fixture.open();
    for account in ["a", "b"] {
        c.put(
            "account",
            account,
            json!({"id":account,"identityVerified":true}),
        )
        .unwrap();
    }
    for id in ["exclusive", "shared", "local:p"] {
        product(&c, id);
    }
    c.put("product", "local:p", json!({"id":"local:p","provider":"local","store":"Local Files","custom":true,"title":"Authored local"})).unwrap();
    for id in ["exclusive", "shared", "local:p"] {
        let acq_id = format!("a-{id}");
        c.put("acquisition", &acq_id, json!({"id":acq_id,"productId":id,"accountId":"a","paidPrice":1200,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"})).unwrap();
        c.put(
            "overlay",
            id,
            json!({"id":id,"note":"Preserved human note"}),
        )
        .unwrap();
    }
    // Sharing must be detected even when the other account has no acquisition.
    c.put("remote_file", "b-shared", json!({"id":"b-shared","productId":"shared","accountId":"b","name":"Other.unitypackage","revision":"r1"})).unwrap();
    c.put("entitlement", "b-shared", json!({"id":"b-shared","productId":"shared","accountId":"b","fileIds":["b-shared"],"manifestComplete":true})).unwrap();
    c.put("remote_file", "a-shared", json!({"id":"a-shared","productId":"shared","accountId":"a","name":"Removed.unitypackage","revision":"r1"})).unwrap();
    c.put("entitlement", "a-shared", json!({"id":"a-shared","productId":"shared","accountId":"a","fileIds":["a-shared"],"manifestComplete":true})).unwrap();
    c.put(
        "acquisition",
        "a-unresolved",
        json!({"id":"a-unresolved","accountId":"a","sourceRecordId":"unresolved"}),
    )
    .unwrap();
    c.put("remote_file", "a-exclusive", json!({"id":"a-exclusive","productId":"exclusive","accountId":"a","name":"Authored.unitypackage","revision":"r1"})).unwrap();
    c.put("entitlement", "a-exclusive", json!({"id":"a-exclusive","productId":"exclusive","accountId":"a","fileIds":["a-exclusive"],"manifestComplete":true})).unwrap();
    c.put(
        "root",
        "r",
        json!({"id":"r","path":fixture.0,"status":"online"}),
    )
    .unwrap();
    for id in ["exclusive", "shared"] {
        let relative = format!("{id}/Authored.unitypackage");
        let path = fixture.0.join(&relative);
        let bytes = b"keep downloaded bytes";
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        use sha2::{Digest, Sha256};
        c.put("artifact", id, json!({"id":id,"productId":id,"fileId":format!("a-{id}"),"rootId":"r","revision":"r1","status":"downloaded","originalAvailable":true,"sizeBytes":bytes.len(),"hash":format!("{:x}",Sha256::digest(bytes)),"relativePath":relative,"productDirectory":id})).unwrap();
    }
    let other_path = fixture.0.join("shared/Other.unitypackage");
    std::fs::write(&other_path, b"other account downloaded bytes").unwrap();
    use sha2::{Digest, Sha256};
    let other_artifact = json!({"id":"b-shared","productId":"shared","fileId":"b-shared","rootId":"r","revision":"r1","status":"ready","originalAvailable":true,"sizeBytes":30,"hash":format!("{:x}",Sha256::digest(b"other account downloaded bytes")),"relativePath":"shared/Other.unitypackage","productDirectory":"shared"});
    c.put("artifact", "b-shared", other_artifact.clone())
        .unwrap();
    c.put(
        "download_intent",
        "exclusive",
        json!({"id":"exclusive","wantDownload":true}),
    )
    .unwrap();
    c.put("job", "history", json!({"id":"history","accountId":"a","productId":"exclusive","type":"download","state":"cancelled"})).unwrap();
    c.put("collection", "manual", json!({"id":"manual","name":"Manual collection","productIds":["exclusive","shared","local:p"]})).unwrap();
    let other_file = c.get("remote_file", "b-shared").unwrap();
    let other_entitlement = c.get("entitlement", "b-shared").unwrap();
    assert_eq!(
        c.remove_account("a", true).unwrap(),
        json!({"accountRemoved":1,"productRemoved":1,"retainedShared":2})
    );
    assert!(c.get("product", "exclusive").unwrap().is_none());
    assert!(c.get("artifact", "exclusive").unwrap().is_none());
    assert!(c.get("download_intent", "exclusive").unwrap().is_none());
    assert!(c.get("account", "b").unwrap().is_some());
    assert!(c.get("product", "shared").unwrap().is_some());
    assert!(c.get("product", "local:p").unwrap().is_some());
    assert_eq!(c.get("remote_file", "b-shared").unwrap(), other_file);
    assert_eq!(c.get("entitlement", "b-shared").unwrap(), other_entitlement);
    assert!(c.get("artifact", "shared").unwrap().is_none());
    assert_eq!(c.get("artifact", "b-shared").unwrap(), Some(other_artifact));
    for kind in ["acquisition", "entitlement", "remote_file"] {
        assert!(!c
            .list(kind)
            .unwrap()
            .iter()
            .any(|row| row["accountId"] == "a"));
    }
    assert!(c.get("overlay", "exclusive").unwrap().is_none());
    for id in ["shared", "local:p"] {
        assert_eq!(
            c.get("overlay", id).unwrap().unwrap()["note"],
            "Preserved human note"
        );
    }
    let details = dynamic_details(&c, "shared");
    assert_eq!(details["accountIds"], json!(["b"]));
    assert_eq!(details["files"].as_array().unwrap().len(), 1);
    assert_eq!(details["files"][0]["id"], "b-shared");
    assert_eq!(details["files"][0]["status"], "ready");
    assert_eq!(
        std::fs::read(other_path).unwrap(),
        b"other account downloaded bytes"
    );
    assert!(details["provenance"].as_array().unwrap().is_empty());
    for id in ["exclusive", "shared"] {
        assert_eq!(
            std::fs::read(fixture.0.join(format!("{id}/Authored.unitypackage"))).unwrap(),
            b"keep downloaded bytes"
        );
    }
    assert_eq!(
        c.get("job", "history").unwrap().unwrap()["state"],
        "cancelled"
    );
    let manifest = c.manifest().unwrap();
    let other = Fixture::new();
    let peer = other.open();
    peer.restore_manifest(manifest).unwrap();
    assert!(peer.get("product", "exclusive").unwrap().is_none());
    assert!(peer.get("overlay", "exclusive").unwrap().is_none());
    assert!(peer.get("product", "shared").unwrap().is_some());
    assert_eq!(
        peer.get("collection", "manual").unwrap().unwrap()["productIds"],
        json!(["shared", "local:p"])
    );
}

#[test]
fn failed_login_discard_guards_success_history_and_active_jobs() {
    let fixture = Fixture::new();
    let c = fixture.open();
    for (id, marker) in [
        ("verified", json!({"identityVerified":true})),
        ("legacy", json!({"authenticatedAt":"2026-10-11T00:00:00Z"})),
        (
            "bound",
            json!({"identityHash":"authored-stable-binding","identityVerified":false}),
        ),
    ] {
        let mut account = marker;
        account["id"] = json!(id);
        c.put("account", id, account).unwrap();
        assert!(!c.discard_unverified_account(id).unwrap());
    }
    for kind in ["acquisition", "entitlement", "remote_file"] {
        c.put("account", kind, json!({"id":kind,"identityVerified":false}))
            .unwrap();
        c.put(kind, kind, json!({"id":kind,"accountId":kind}))
            .unwrap();
        assert!(!c.discard_unverified_account(kind).unwrap());
    }
    for state in ["queued", "running"] {
        c.put(
            "account",
            state,
            json!({"id":state,"identityVerified":false}),
        )
        .unwrap();
        c.put(
            "job",
            state,
            json!({"id":state,"accountId":state,"type":"boothLogin","state":state}),
        )
        .unwrap();
        assert!(!c.discard_unverified_account(state).unwrap());
        c.put(
            "job",
            state,
            json!({"id":state,"accountId":state,"type":"boothLogin","state":"cancelled"}),
        )
        .unwrap();
        assert!(c.discard_unverified_account(state).unwrap());
        assert_eq!(c.get("job", state).unwrap().unwrap()["state"], "cancelled");
    }
    c.put(
        "account",
        "draft",
        json!({"id":"draft","identityVerified":false,"authenticatedAt":null}),
    )
    .unwrap();
    c.put("job", "failed", json!({"id":"failed","accountId":"draft","type":"boothLogin","state":"failed","errorCode":"AUTH_REQUIRED"})).unwrap();
    assert!(c.discard_unverified_account("draft").unwrap());
    assert_eq!(
        c.get("job", "failed").unwrap().unwrap()["errorCode"],
        "AUTH_REQUIRED"
    );
    assert!(!c.discard_unverified_account("draft").unwrap());
}

#[test]
fn account_counts_include_hidden_products_and_deduplicate_observations() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["a", "b", "zero"] {
        c.put("account", id, json!({"id":id,"store":"BOOTH"}))
            .unwrap();
    }
    for (id, category) in [
        ("hair", "Hair"),
        ("other", "NonVRChat"),
        ("unassigned", "NonVRChat"),
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"title":id,"category":category}),
        )
        .unwrap();
    }
    c.put(
        "product",
        "custom",
        json!({"id":"custom","title":"Custom","category":"NonVRChat","custom":true}),
    )
    .unwrap();
    for (id, product, account) in [
        ("a-one", "hair", "a"),
        ("a-two", "hair", "a"),
        ("a-other", "other", "a"),
        ("b-one", "hair", "b"),
    ] {
        c.put(
            "acquisition",
            id,
            json!({"id":id,"productId":product,"accountId":account,"source":"purchase"}),
        )
        .unwrap();
    }
    for id in ["file-one", "file-two"] {
        c.put(
            "remote_file",
            id,
            json!({"id":id,"productId":"hair","accountId":"a","name":id,"revision":"r1"}),
        )
        .unwrap();
    }
    let before = c.change_domains().unwrap();
    let expected = json!({"accounts":{"a":2,"b":1,"zero":0},"unassigned":1});
    assert_eq!(c.command("account_counts", json!({})).unwrap(), expected);
    let visible = c.command("query", json!({"includeFiles":false})).unwrap();
    assert!(!strings(&visible["matchedIds"]).contains(&"other".to_owned()));
    c.command(
        "query",
        json!({"category":"NonVRChat","search":"other","includeFiles":false}),
    )
    .unwrap();
    assert_eq!(c.command("account_counts", json!({})).unwrap(), expected);
    assert_eq!(c.change_domains().unwrap(), before);
}

#[test]
fn clear_activity_removes_only_terminal_jobs_and_preserves_current_facts() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "p");
    let account = json!({"id":"a","store":"BOOTH","lastSyncAt":"2026-10-11T00:00:00Z"});
    let checkpoint = json!({"id":"restart","accountId":"a","page":3});
    let artifact =
        json!({"id":"artifact","productId":"p","fileId":"f","status":"missing","revision":"r"});
    c.put("account", "a", account.clone()).unwrap();
    c.put("checkpoint", "restart", checkpoint.clone()).unwrap();
    c.put("artifact", "artifact", artifact.clone()).unwrap();
    for state in [
        "queued",
        "running",
        "completed",
        "failed",
        "needsAction",
        "cancelled",
    ] {
        c.put(
            "job",
            state,
            json!({"id":state,"type":"download","productId":"p","state":state}),
        )
        .unwrap();
    }
    let result = c.command("clear_activity", json!({})).unwrap();
    assert_eq!(result, json!({"cleared":4,"remainingActive":2}));
    assert_eq!(c.list("job").unwrap().len(), 2);
    assert!(c.get("job", "queued").unwrap().is_some());
    assert!(c.get("job", "running").unwrap().is_some());
    assert_eq!(c.get("account", "a").unwrap(), Some(account));
    assert_eq!(c.get("checkpoint", "restart").unwrap(), Some(checkpoint));
    assert_eq!(c.get("artifact", "artifact").unwrap(), Some(artifact));
    assert!(c.get("product", "p").unwrap().is_some());
    assert_eq!(
        c.command("clear_activity", json!({})).unwrap()["cleared"],
        0
    );
}
