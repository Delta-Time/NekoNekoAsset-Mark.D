//! Library roots, migration, export, backup and restore.
use super::*;

#[test]
fn preferences_and_revision_identity_are_deterministic() {
    assert_eq!(download_count(&Value::Null), 3);
    assert_eq!(download_count(&json!(25)), 6);
    assert_eq!(download_count(&json!(0)), 1);
    assert_eq!(download_count(&json!(7)), 6);
    assert_eq!(download_count(&json!(4)), 4);
    let a = json!({"id":"f","revision":"r1","sourceUrl":"https://booth.pm/x"});
    let mut b = a.clone();
    b["sourceUrl"] = json!("https://booth.pm/y");
    assert_eq!(artifact_id(&a), artifact_id(&b));
    b["revision"] = json!("r2");
    assert_ne!(artifact_id(&a), artifact_id(&b));
}

#[test]
fn migration_copy_verify_switch_retains_original_until_explicit_cleanup() {
    let f = Fixture::new();
    f.seed(1, true);
    f.artifact(0);
    let old = f
        .path
        .join("library/assets")
        .join(io::readable_component("Synthetic clothing", "42"))
        .join("fixture-0/fixture-0.unitypackage");
    let new = f.path.join("new-library");
    f.service
        .disk_operation(
            "migrate_root",
            &json!({"rootId":"test-root","path":new}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(old.exists());
    assert!(new
        .join("assets")
        .join(io::readable_component("Synthetic clothing", "42"))
        .join("fixture-0/fixture-0.unitypackage")
        .exists());
    assert_eq!(
        PathBuf::from(
            f.service.root(Some("test-root")).unwrap()["path"]
                .as_str()
                .unwrap()
        )
        .canonicalize()
        .unwrap(),
        new.canonicalize().unwrap()
    );
    std::fs::write(&old, b"locally edited").unwrap();
    assert!(f
        .service
        .disk_operation(
            "cleanup_old_root",
            &json!({"rootId":"test-root"}),
            &AtomicBool::new(false)
        )
        .is_err());
    assert!(old.exists());
}

#[test]
fn remap_and_migration_cancellation_never_switch_to_unverified_destination() {
    let f = Fixture::new();
    f.seed(1, true);
    f.artifact(0);
    let destination = f.path.join("new-library");
    std::fs::create_dir_all(&destination).unwrap();
    assert!(f
        .service
        .disk_operation(
            "remap_root",
            &json!({"rootId":"test-root","path":destination}),
            &AtomicBool::new(false)
        )
        .is_err());
    assert!(f
        .service
        .disk_operation(
            "migrate_root",
            &json!({"rootId":"test-root","path":destination}),
            &AtomicBool::new(true)
        )
        .is_err());
    assert_eq!(
        f.service.root(Some("test-root")).unwrap()["path"],
        json!(f.path.join("library"))
    );
}

#[test]
fn incomplete_product_cannot_export_and_path_traversal_is_refused() {
    let f = Fixture::new();
    f.seed(2, true);
    f.artifact(0);
    assert!(f
        .service
        .disk_operation(
            "export_zip",
            &json!({"productIds":["42"],"path":f.path.join("export.zip")}),
            &AtomicBool::new(false)
        )
        .is_err());
    assert!(!f.path.join("export.zip").exists());
    assert!(io::safe_join(&f.path.join("library"), "../outside").is_err());
    assert!(io::safe_join(&f.path.join("library"), "a:stream").is_err());
}

#[test]
fn observed_ready_files_export_before_source_scan_closes() {
    let f = Fixture::new();
    f.seed(1, false);
    f.artifact(0);
    let destination = f.path.join("observed-ready.zip");
    let result = f
        .service
        .disk_operation(
            "export_zip",
            &json!({"productIds":["42"],"path":destination}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(result["fileCount"], 1);
    assert!(destination.exists());
    assert_eq!(
        f.service
            .catalogue
            .command("product_details", json!({"id":"42"}))
            .unwrap()["manifestComplete"],
        false
    );
}

#[test]
fn kept_file_whose_extraction_failed_still_exports_its_original() {
    let f = Fixture::new();
    f.seed(2, true);
    let current = f.artifact(0);
    let mut kept = f.artifact(1);
    // The store dropped the second file after its download; its extraction had failed.
    f.service.catalogue.put("entitlement","test-entitlement",json!({"id":"test-entitlement","productId":"42","accountId":"test-account","fileIds":["test-file-0"],"manifestComplete":true})).unwrap();
    kept["status"] = json!("extractionFailed");
    kept["errorCode"] = json!("archive_corrupt");
    let id = kept["id"].as_str().unwrap().to_owned();
    f.service.catalogue.put("artifact", &id, kept).unwrap();
    let result = f
        .service
        .disk_operation(
            "export_zip",
            &json!({"productIds":["42"],"path":f.path.join("kept.zip")}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(result["fileCount"], 2);
    // A file its store refused counts the same while a grant still names it.
    f.service.catalogue.put("entitlement","test-entitlement",json!({"id":"test-entitlement","productId":"42","accountId":"test-account","fileIds":["test-file-0","test-file-1"],"manifestComplete":true})).unwrap();
    // The record a refused download leaves, as the transfer writes it.
    let pending = format!("pending-{}", io::key("test-file-1"));
    f.service.catalogue.put("artifact",&pending,json!({"id":pending,"productId":"42","fileId":"test-file-1","rootId":"test-root","relativePath":"assets/refused/fixture-1.unitypackage","productDirectory":"assets/refused","revision":null,"status":"notDownloaded","errorCode":"store_file_unavailable","originalAvailable":false})).unwrap();
    let result = f
        .service
        .disk_operation(
            "export_zip",
            &json!({"productIds":["42"],"path":f.path.join("refused.zip")}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(result["fileCount"], 2);
    // The store's own files must still be ready.
    let mut failed = current;
    failed["status"] = json!("extractionFailed");
    let id = failed["id"].as_str().unwrap().to_owned();
    f.service.catalogue.put("artifact", &id, failed).unwrap();
    assert!(f
        .service
        .disk_operation(
            "export_zip",
            &json!({"productIds":["42"],"path":f.path.join("current.zip")}),
            &AtomicBool::new(false),
        )
        .is_err());
}

#[test]
fn metadata_restore_originals_recover_only_after_verified_remap() {
    let source = Fixture::new();
    source.seed(1, true);
    let original = source.artifact(0);
    let manifest = source.service.catalogue.manifest().unwrap();
    let restored = Fixture::new();
    restored
        .service
        .catalogue
        .restore_manifest(manifest.clone())
        .unwrap();
    let id = original["id"].as_str().unwrap();
    let unknown = restored
        .service
        .catalogue
        .get("artifact", id)
        .unwrap()
        .unwrap();
    assert_eq!(unknown["status"], "unknown");
    assert_eq!(unknown["restoreStatus"], "ready");
    assert_eq!(unknown["originalAvailable"], false);
    let old_root = restored.service.root(Some("test-root")).unwrap();
    let destination = restored.path.join("mapped-library");
    let target = destination.join(original["relativePath"].as_str().unwrap());
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    // Same size, different bytes: path and size alone cannot restore Ready.
    std::fs::write(&target, b"synthetic unity package bytez").unwrap();
    let args = json!({"rootId":"test-root","path":destination});
    assert!(restored
        .service
        .disk_operation("remap_root", &args, &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        restored.service.root(Some("test-root")).unwrap()["path"],
        old_root["path"]
    );
    assert_eq!(
        restored
            .service
            .catalogue
            .get("artifact", id)
            .unwrap()
            .unwrap(),
        unknown
    );
    assert!(restored
        .service
        .resolve_open_target("open_file", &json!({"fileId":"test-file-0"}))
        .is_err());
    std::fs::write(&target, b"synthetic unity package bytes").unwrap();
    restored
        .service
        .disk_operation("remap_root", &args, &AtomicBool::new(false))
        .unwrap();
    let recovered = restored
        .service
        .catalogue
        .get("artifact", id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered["status"], "ready");
    assert_eq!(recovered["originalAvailable"], true);
    assert!(recovered.get("restoreStatus").is_none());
    assert_eq!(
        restored
            .service
            .resolve_open_target("open_file", &json!({"fileId":"test-file-0"}))
            .unwrap()
            .canonicalize()
            .unwrap(),
        target.canonicalize().unwrap()
    );
}

#[test]
fn metadata_restore_extracted_state_requires_verified_members_and_truthful_original_availability() {
    for retained in [false, true] {
        let source = Fixture::new();
        source.seed(1, true);
        let (artifact, original, source_tree) = source.extracted_archive(0, retained);
        let manifest = source.service.catalogue.manifest().unwrap();
        let restored = Fixture::new();
        restored
            .service
            .catalogue
            .restore_manifest(manifest)
            .unwrap();
        let id = artifact["id"].as_str().unwrap();
        let unknown = restored
            .service
            .catalogue
            .get("artifact", id)
            .unwrap()
            .unwrap();
        assert_eq!(unknown["status"], "unknown");
        assert_eq!(unknown["restoreStatus"], "ready");
        assert_eq!(unknown["originalAvailable"], false);
        let old_root = restored.service.root(Some("test-root")).unwrap();
        let destination = restored.path.join("mapped-library");
        let tree = destination.join(artifact["extractedPath"].as_str().unwrap());
        std::fs::create_dir_all(tree.join("empty")).unwrap();
        let member = tree.join("model.fbx");
        let correct = std::fs::read(source_tree.join("model.fbx")).unwrap();
        let mut damaged = correct.clone();
        damaged[0] ^= 1;
        std::fs::write(&member, &damaged).unwrap();
        let restored_original = destination.join(artifact["relativePath"].as_str().unwrap());
        if retained {
            std::fs::create_dir_all(restored_original.parent().unwrap()).unwrap();
            std::fs::copy(&original, &restored_original).unwrap();
        }
        let args = json!({"rootId":"test-root","path":destination});
        assert!(restored
            .service
            .disk_operation("remap_root", &args, &AtomicBool::new(false))
            .is_err());
        assert_eq!(
            restored.service.root(Some("test-root")).unwrap()["path"],
            old_root["path"]
        );
        assert_eq!(
            restored
                .service
                .catalogue
                .get("artifact", id)
                .unwrap()
                .unwrap(),
            unknown
        );
        assert!(restored
            .service
            .resolve_open_target("open_file", &json!({"fileId":"test-file-0"}))
            .is_err());
        assert_eq!(std::fs::read(&member).unwrap(), damaged);
        std::fs::write(&member, &correct).unwrap();
        restored
            .service
            .disk_operation("remap_root", &args, &AtomicBool::new(false))
            .unwrap();
        let recovered = restored
            .service
            .catalogue
            .get("artifact", id)
            .unwrap()
            .unwrap();
        assert_eq!(recovered["status"], "ready");
        assert_eq!(recovered["originalAvailable"], retained);
        assert!(recovered.get("restoreStatus").is_none());
        assert_eq!(recovered["relativePath"], artifact["relativePath"]);
        assert_eq!(recovered["extractedPath"], artifact["extractedPath"]);
        assert_eq!(
            recovered["extractedInventory"],
            artifact["extractedInventory"]
        );
        assert_eq!(restored_original.exists(), retained);
        if retained {
            assert_eq!(
                std::fs::read(&restored_original).unwrap(),
                std::fs::read(&original).unwrap()
            );
        }
        assert_eq!(
            restored
                .service
                .resolve_open_target("open_file", &json!({"fileId":"test-file-0"}))
                .unwrap()
                .canonicalize()
                .unwrap(),
            tree.canonicalize().unwrap()
        );
    }
}

#[test]
fn full_backup_restore_preserves_files_empty_dirs_media_and_human_metadata() {
    let f = Fixture::new();
    f.seed(1, true);
    let mut a = f.artifact(0);
    let extracted = f.path.join("library/extracted/example");
    std::fs::create_dir_all(extracted.join("empty")).unwrap();
    std::fs::write(extracted.join("source.psd"), b"synthetic pixels").unwrap();
    let (hash, size) = io::hash_file(&extracted.join("source.psd")).unwrap();
    a["extractedPath"] = json!("extracted/example");
    a["extractedInventory"] = json!([{"relativePath":"empty","isDirectory":true,"hash":"","sizeBytes":0},{"relativePath":"source.psd","hash":hash,"sizeBytes":size,"isDirectory":false}]);
    f.service
        .catalogue
        .put("artifact", a["id"].as_str().unwrap(), a.clone())
        .unwrap();
    let media = f.path.join("data/media/fixture.png");
    std::fs::create_dir_all(media.parent().unwrap()).unwrap();
    std::fs::write(&media, b"independently authored test image payload").unwrap();
    f.service.catalogue.put("overlay","42",json!({"id":"42","title":"User name","note":"User note","rejectedTags":["rejected"],"thumbnailPath":"media/fixture.png"})).unwrap();
    f.service
        .catalogue
        .put(
            "acquisition",
            "acq",
            json!({"id":"acq","productId":"42","accountId":"test-account","source":"gift"}),
        )
        .unwrap();
    let backup = f.path.join("backup.nna-backup.zip");
    f.service
        .disk_operation(
            "export_backup",
            &json!({"path":backup}),
            &AtomicBool::new(false),
        )
        .unwrap();
    let destination = f.path.join("restored");
    std::fs::create_dir_all(&destination).unwrap();
    f.service
        .disk_operation(
            "restore_backup",
            &json!({"path":backup,"destinationPath":destination}),
            &AtomicBool::new(false),
        )
        .unwrap();
    let overlay = f.service.catalogue.get("overlay", "42").unwrap().unwrap();
    assert_eq!(overlay["title"], "User name");
    assert_eq!(overlay["rejectedTags"], json!(["rejected"]));
    assert!(f
        .path
        .join("data")
        .join(overlay["thumbnailPath"].as_str().unwrap())
        .exists());
    assert!(f
        .service
        .catalogue
        .get("acquisition", "acq")
        .unwrap()
        .is_some());
    assert!(f
        .service
        .catalogue
        .get("entitlement", "test-entitlement")
        .unwrap()
        .is_some());
    let root = f.service.root(Some("test-root")).unwrap();
    let root = PathBuf::from(root["path"].as_str().unwrap());
    assert!(root.starts_with(&destination));
    assert!(root.join("extracted/example/empty").is_dir());
    let restored = f
        .service
        .catalogue
        .get("artifact", a["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(restored["status"], "ready");
    assert_eq!(restored["originalAvailable"], true);
}

#[test]
fn backup_export_refuses_missing_current_or_conflict_thumbnail_payloads() {
    let f = Fixture::new();
    f.seed(1, true);
    f.artifact(0);
    f.service
        .catalogue
        .put(
            "overlay",
            "42",
            json!({"id":"42","thumbnailPath":"media/missing.png"}),
        )
        .unwrap();
    let backup = f.path.join("missing-media.zip");
    let error = f
        .service
        .disk_operation(
            "export_backup",
            &json!({"path":backup}),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(error.to_string().contains("backup_payload_missing"));
    assert!(!backup.exists());
    let mut manifest = f.service.catalogue.manifest().unwrap();
    manifest["records"]["overlay"][0]
        .as_object_mut()
        .unwrap()
        .remove("thumbnailPath");
    manifest["media"] = json!(["media/retained-conflict.png"]);
    assert!(f.service.backup_payload(&manifest).is_err());
}

#[test]
fn backup_export_uses_captured_artifacts_after_the_live_records_change() {
    let f = Fixture::new();
    f.seed(1, true);
    let artifact = f.artifact(0);
    let manifest = f.service.catalogue.manifest().unwrap();
    f.service
        .catalogue
        .delete("artifact", artifact["id"].as_str().unwrap())
        .unwrap();
    let payload = f.service.backup_payload(&manifest).unwrap();
    assert_eq!(payload.len(), 1);
    let backup = f.path.join("captured-backup.zip");
    portable::create_backup(&backup, manifest, &payload, &AtomicBool::new(false)).unwrap();
    let restored = portable::restore_backup(
        &backup,
        &f.path.join("captured-restore"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(restored.payload.len(), 1);
    assert_eq!(restored.catalogue["records"]["artifact"][0], artifact);
}

#[test]
fn backup_restore_keeps_only_explicit_running_local_restore_job_observable() {
    let f = Fixture::new();
    f.seed(1, true);
    f.artifact(0);
    let catalogue = &f.service.catalogue;
    catalogue
        .put(
            "job",
            "other-device-restore",
            json!({"id":"other-device-restore","type":"storage:restore_backup","state":"running"}),
        )
        .unwrap();
    catalogue
        .put(
            "job",
            "old-download",
            json!({"id":"old-download","type":"download","state":"completed"}),
        )
        .unwrap();
    catalogue
        .put(
            "checkpoint",
            "old-checkpoint",
            json!({"id":"old-checkpoint"}),
        )
        .unwrap();
    catalogue
        .put("download_intent", "42", json!({"id":"42","selected":true}))
        .unwrap();
    let backup = f.path.join("active-job-backup.zip");
    f.service
        .disk_operation(
            "export_backup",
            &json!({"path":backup}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(catalogue.manifest().unwrap()["records"]
        .get("job")
        .is_none());
    let active = json!({"id":"this-device-restore","type":"storage:restore_backup","state":"running","createdAt":"2026-10-10T00:00:00Z","updatedAt":"2026-10-10T00:00:01Z"});
    catalogue
        .put("job", "this-device-restore", active.clone())
        .unwrap();
    f.service
        .restore(
            &json!({"path":backup,"destinationPath":f.path.join("active-job-restored")}),
            &AtomicBool::new(false),
            Some("this-device-restore"),
        )
        .unwrap();
    // Observe immediately after atomic replacement, before launch_operation can
    // re-record completion. The active job must never require that later write.
    assert_eq!(
        catalogue.command("state", json!({})).unwrap()["jobs"],
        json!([active])
    );
    assert!(catalogue
        .get("job", "other-device-restore")
        .unwrap()
        .is_none());
    assert!(catalogue.get("job", "old-download").unwrap().is_none());
    assert!(catalogue.list("checkpoint").unwrap().is_empty());
    assert!(catalogue.list("download_intent").unwrap().is_empty());
    // Without an explicit local owner, ordinary restore retains no old jobs.
    f.service
        .disk_operation(
            "restore_backup",
            &json!({"path":backup,"destinationPath":f.path.join("ordinary-restored")}),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(catalogue.list("job").unwrap().is_empty());
}

#[test]
fn backup_manifest_omitted_ready_payload_cannot_replace_catalogue() {
    let f = Fixture::new();
    f.seed(1, true);
    let a = f.artifact(0);
    let manifest = f.service.catalogue.manifest().unwrap();
    let backup = f.path.join("incomplete.zip");
    portable::create_backup(&backup, manifest, &[], &AtomicBool::new(false)).unwrap();
    assert!(f
        .service
        .disk_operation(
            "restore_backup",
            &json!({"path":backup,"destinationPath":f.path.join("restore")}),
            &AtomicBool::new(false)
        )
        .is_err());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", a["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        a
    );
    assert_eq!(
        f.service.root(Some("test-root")).unwrap()["path"],
        json!(f.path.join("library"))
    );
}
