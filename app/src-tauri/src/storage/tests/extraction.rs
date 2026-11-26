//! Archive extraction, verification, open targets and managed paths.
use super::*;

#[test]
fn cleaned_archives_open_their_own_verified_extraction_and_product_folder_spans_variants() {
    let f = Fixture::new();
    f.seed(2, true);
    let (_, _, first) = f.extracted_archive(0, false);
    let (_, _, second) = f.extracted_archive(1, false);
    for (id, expected) in [("test-file-0", first), ("test-file-1", second)] {
        assert_eq!(
            f.service
                .resolve_open_target("open_file", &json!({"fileId":id}))
                .unwrap(),
            expected
        );
    }
    let expected = f
        .path
        .join("library/assets")
        .join(io::readable_component("Synthetic clothing", "42"));
    assert_eq!(
        f.service
            .resolve_open_target("open_product_folder", &json!({"productId":"42"}))
            .unwrap(),
        expected
    );
    let availability = f
        .service
        .product_availability(&json!({"productId":"42"}))
        .unwrap();
    assert_eq!(availability["folderAvailable"], true);
    let ids = strings(&availability["openableFileIds"]);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"test-file-0".to_owned()) && ids.contains(&"test-file-1".to_owned()));
}

#[test]
fn archive_open_prefers_completed_directory_and_integrity_remains_explicit() {
    let f = Fixture::new();
    f.seed(1, true);
    let (artifact, original, extracted) = f.extracted_archive(0, true);
    let args = json!({"fileId":"test-file-0"});
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    std::fs::write(&original, b"corrupt original bytes").unwrap();
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    assert_eq!(
        strings(
            &f.service
                .product_availability(&json!({"productId":"42"}))
                .unwrap()["openableFileIds"]
        ),
        vec!["test-file-0".to_owned()]
    );
    // Opening a completed directory does not read its payload. Explicit
    // verification continues to detect edits and missing members.
    std::fs::write(extracted.join("model.fbx"), b"modified synthetic model").unwrap();
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    // The original is a fallback only when the extracted directory is absent.
    std::fs::write(&original, b"synthetic complete archive").unwrap();
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    std::fs::remove_file(extracted.join("model.fbx")).unwrap();
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    assert_eq!(
        strings(
            &f.service
                .product_availability(&json!({"productId":"42"}))
                .unwrap()["openableFileIds"]
        ),
        vec!["test-file-0".to_owned()]
    );
    std::fs::write(extracted.join("model.fbx"), b"verified synthetic model").unwrap();
    let moved = extracted.with_extension("moved");
    std::fs::rename(&extracted, &moved).unwrap();
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        original
    );
    std::fs::rename(&moved, &extracted).unwrap();
    // A stale availability flag after cleanup does not hide a verified usable extraction.
    std::fs::remove_file(original).unwrap();
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    // Non-archive files continue to open the verified retained file itself.
    let artifact = f.artifact(0);
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        f.path
            .join("library")
            .join(artifact["relativePath"].as_str().unwrap())
    );
}

#[test]
fn extracted_target_requires_completed_state_and_explicit_verification_rejects_bad_inventory() {
    let f = Fixture::new();
    f.seed(1, true);
    let (mut artifact, _, extracted) = f.extracted_archive(0, false);
    let args = json!({"fileId":"test-file-0"});
    let product = json!({"productId":"42"});
    let id = artifact["id"].as_str().unwrap().to_owned();
    for status in [
        "queued",
        "downloading",
        "extracting",
        "unknown",
        "missing",
        "extractionFailed",
    ] {
        artifact["status"] = json!(status);
        f.service
            .catalogue
            .put("artifact", &id, artifact.clone())
            .unwrap();
        assert!(f.service.resolve_open_target("open_file", &args).is_err());
        assert!(
            strings(&f.service.product_availability(&product).unwrap()["openableFileIds"])
                .is_empty()
        );
    }
    artifact["status"] = json!("ready");
    f.service
        .catalogue
        .put("artifact", &id, artifact.clone())
        .unwrap();
    // Same-size edits are checked on explicit verification, not each directory open.
    std::fs::write(extracted.join("model.fbx"), b"modified synthetic model").unwrap();
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service.resolve_open_target("open_file", &args).unwrap(),
        extracted
    );
    std::fs::remove_file(extracted.join("model.fbx")).unwrap();
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        strings(&f.service.product_availability(&product).unwrap()["openableFileIds"]),
        vec!["test-file-0"]
    );
    artifact["extractedInventory"] = json!([]);
    f.service
        .catalogue
        .put("artifact", &id, artifact.clone())
        .unwrap();
    assert!(f.service.resolve_open_target("open_file", &args).is_err());
    artifact["extractedInventory"] =
        json!([{"relativePath":"../escape.bin","hash":io::key("escape"),"sizeBytes":6}]);
    // Supplemental inventory is accepted as metadata. The content verifier
    // owns its member-path check; opening consumes only the safe base path.
    f.service
        .catalogue
        .put("artifact", &id, artifact.clone())
        .unwrap();
    let before = f.service.catalogue.get("artifact", &id).unwrap().unwrap();
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
    // The folder action doesn't consume inventory paths. Its own target must
    // still stay contained in the mapped root.
    artifact["extractedInventory"] =
        json!([{"relativePath":"model.fbx","hash":io::key("escape"),"sizeBytes":6}]);
    artifact["extractedPath"] = json!("../escape");
    let error = f
        .service
        .catalogue
        .put("artifact", &id, artifact.clone())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("catalogue.invalid_relative_path"));
    assert_eq!(
        f.service.catalogue.get("artifact", &id).unwrap().unwrap(),
        before
    );
    assert!(f
        .service
        .verify_extracted_at(&artifact, &f.path.join("library"), &AtomicBool::new(false))
        .is_err());
}

#[test]
fn product_folder_requires_recorded_containing_namespace_without_guessing_paths() {
    let f = Fixture::new();
    f.seed(1, true);
    let (mut artifact, _, extracted) = f.extracted_archive(0, false);
    let expected = f
        .path
        .join("library/assets")
        .join(io::readable_component("Synthetic clothing", "42"));
    assert_eq!(
        f.service
            .resolve_open_target("open_product_folder", &json!({"productId":"42"}))
            .unwrap(),
        expected
    );
    assert_eq!(
        f.service
            .resolve_open_target("open_file", &json!({"fileId":"test-file-0"}))
            .unwrap(),
        extracted
    );
    artifact["productDirectory"] = json!("assets/unrelated-product");
    f.service
        .catalogue
        .put(
            "artifact",
            artifact["id"].as_str().unwrap(),
            artifact.clone(),
        )
        .unwrap();
    assert!(f
        .service
        .resolve_open_target("open_product_folder", &json!({"productId":"42"}))
        .is_err());
    assert_eq!(
        f.service
            .product_availability(&json!({"productId":"42"}))
            .unwrap()["folderAvailable"],
        false
    );
}

#[test]
fn open_targets_follow_current_root_mapping_and_refuse_offline_or_unknown_revision() {
    let f = Fixture::new();
    f.seed(1, true);
    let (_, _, extracted) = f.extracted_archive(0, false);
    let canonical = f.path.join("library").canonicalize().unwrap();
    f.service
        .catalogue
        .put(
            "root",
            "test-root",
            json!({"id":"test-root","path":canonical,"status":"online"}),
        )
        .unwrap();
    let args = json!({"fileId":"test-file-0"});
    assert_eq!(
        f.service
            .resolve_open_target("open_file", &args)
            .unwrap()
            .canonicalize()
            .unwrap(),
        extracted.canonicalize().unwrap()
    );
    std::fs::rename(f.path.join("library"), f.path.join("offline-library")).unwrap();
    assert!(f.service.resolve_open_target("open_file", &args).is_err());
    assert_eq!(
        f.service
            .product_availability(&json!({"productId":"42"}))
            .unwrap()["rootStatus"],
        "offline"
    );
    std::fs::rename(f.path.join("offline-library"), f.path.join("library")).unwrap();
    let mut remote = f
        .service
        .catalogue
        .get("remote_file", "test-file-0")
        .unwrap()
        .unwrap();
    remote["revision"] = json!("unknown");
    f.service
        .catalogue
        .put("remote_file", "test-file-0", remote)
        .unwrap();
    assert!(f.service.resolve_open_target("open_file", &args).is_err());
    assert!(strings(
        &f.service
            .product_availability(&json!({"productId":"42"}))
            .unwrap()["openableFileIds"]
    )
    .is_empty());
}

#[test]
fn readable_managed_paths_are_contained_collision_safe_and_partial_cleanup_is_owned() {
    let f = Fixture::new();
    let root = f.path.join("library");
    for label in ["衣装 / shader:*", "CON.txt", "..", ""] {
        let component = io::readable_component(label, "synthetic-product");
        archive::validate_relative_path(&component).unwrap();
        assert!(!component.contains('/'));
    }
    let directory = format!(
        "assets/{}",
        io::readable_component("Synthetic outfit", "42")
    );
    let first = io::unused_relative(&root, &directory).unwrap();
    std::fs::create_dir_all(io::safe_join(&root, &first).unwrap()).unwrap();
    assert_ne!(io::unused_relative(&root, &directory).unwrap(), first);
    assert!(io::unused_relative(&root, "../outside").is_err());
    let originals = root.join("retained.zip");
    std::fs::write(&originals, b"completed original").unwrap();
    let partials = root.join(".nna-partials");
    std::fs::create_dir(&partials).unwrap();
    let owned = partials.join("pending-owned.part");
    let sibling = partials.join("pending-sibling.part");
    std::fs::write(&owned, b"incomplete owned bytes").unwrap();
    std::fs::write(&sibling, b"other operation").unwrap();
    {
        let _guard = io::PartialGuard::new(owned.clone());
    }
    assert!(!owned.exists());
    assert_eq!(std::fs::read(&sibling).unwrap(), b"other operation");
    assert_eq!(std::fs::read(&originals).unwrap(), b"completed original");
    {
        let _guard = io::PartialGuard::new(sibling);
    }
    assert!(!partials.exists());
    let empty_source = root.join(&directory).join("revision/original");
    let boundary = root.join(&directory);
    std::fs::create_dir_all(&empty_source).unwrap();
    io::remove_empty_parents(&empty_source, &boundary);
    assert!(!empty_source.exists());
    assert!(!boundary.join("revision").exists());
    assert!(boundary.is_dir());
    let guarded = boundary.join("retained/original");
    std::fs::create_dir_all(&guarded).unwrap();
    std::fs::write(guarded.join("manual.txt"), b"user retained sibling").unwrap();
    io::remove_empty_parents(&guarded, &boundary);
    assert_eq!(
        std::fs::read(guarded.join("manual.txt")).unwrap(),
        b"user retained sibling"
    );
    let outside = f.path.join("outside");
    std::fs::create_dir(&outside).unwrap();
    io::remove_empty_parents(&outside, &boundary);
    assert!(outside.is_dir());
}

#[test]
fn store_names_windows_cannot_store_map_to_stable_readable_names() {
    for (name, managed) in [
        ("Outfit.zip", "Outfit.zip"),
        ("衣装 v1.2.unitypackage", "衣装 v1.2.unitypackage"),
        ("Outfit: Summer?.zip", "Outfit： Summer？.zip"),
        ("a<b>c\"d|e*f.txt", "a＜b＞c＂d｜e＊f.txt"),
        ("../file\\name.zip", "..／file＼name.zip"),
        ("tab\there.txt", "tab_here.txt"),
        ("Asset.zip. ", "Asset.zip"),
        ("CON", "_CON"),
        ("nul.txt", "_nul.txt"),
        ("..", "_"),
    ] {
        // Valid names are unchanged, so existing downloads keep their paths.
        assert_eq!(io::managed_name(name), managed, "{name}");
        let path = archive::validate_relative_path(managed).unwrap();
        assert_eq!(path.components().count(), 1);
    }
    // The extension survives, so a mapped archive is still extracted.
    assert!(archive::is_supported_archive(Path::new(&io::managed_name(
        "Outfit: Summer?.zip"
    ))));
}

#[test]
fn manifest_fingerprint_tracks_revalidation_and_file_delta_independent_of_order() {
    let a = json!({"id":"a","revision":"r1"});
    let b = json!({"id":"b","revision":"r2"});
    assert_eq!(
        fingerprint(&[a.clone(), b.clone()]),
        fingerprint(&[b.clone(), a.clone()])
    );
    let mut refresh = a.clone();
    refresh["requiresContentVerification"] = json!(true);
    assert_ne!(fingerprint(&[a.clone()]), fingerprint(&[refresh]));
    assert_ne!(fingerprint(&[a.clone()]), fingerprint(&[a, b]));
}

#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
async fn extraction_group_does_not_hash_unrelated_corrupt_originals() {
    use std::io::Write;
    let tool = native_seven_zip_test_tool();
    let f = Fixture::new();
    f.seed(2, true);
    let (mut artifact, original, _) = f.extracted_archive(0, true);
    let (_, unrelated, _) = f.extracted_archive(1, true);
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&original).unwrap());
    archive
        .start_file(
            "synthetic.unitypackage",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"synthetic intact package").unwrap();
    archive.finish().unwrap();
    let (hash, size) = io::hash_file(&original).unwrap();
    artifact["hash"] = json!(hash);
    artifact["sizeBytes"] = json!(size);
    artifact["status"] = json!("downloaded");
    artifact.as_object_mut().unwrap().remove("extractedPath");
    artifact
        .as_object_mut()
        .unwrap()
        .remove("extractedInventory");
    f.service
        .catalogue
        .put(
            "artifact",
            artifact["id"].as_str().unwrap(),
            artifact.clone(),
        )
        .unwrap();
    // A different archive in this same account and variant must not become
    // a blocker, or even have its bytes read by this extraction attempt.
    std::fs::write(&unrelated, b"edited unrelated original").unwrap();
    let files = f.service.remote_files("42").unwrap().0;
    let root = f.service.root(Some("test-root")).unwrap();
    f.service
        .extract_for_download(
            &tool,
            &root,
            &mut artifact,
            &files,
            &Arc::new(AtomicBool::new(false)),
            &Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(artifact["status"], "ready");
    assert_eq!(artifact["originalAvailable"], false);
    assert!(!original.exists());
    assert!(!original.parent().unwrap().exists());
    assert!(!original.parent().unwrap().parent().unwrap().exists());
    assert!(!f.path.join("library/.nna-staging").exists());
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"edited unrelated original"
    );
    let extracted = io::safe_join(
        &f.path.join("library"),
        artifact["extractedPath"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(extracted.join("synthetic.unitypackage")).unwrap(),
        b"synthetic intact package"
    );
}

#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
async fn verified_downloads_real_extraction_certification_and_reopen_preserve_ready_and_owned_cleanup(
) {
    let tool = native_seven_zip_test_tool();
    let f = Fixture::new();
    f.seed(3, false);
    let mut product = f.service.catalogue.get("product", "42").unwrap().unwrap();
    product["store"] = json!("BOOTH");
    product["provider"] = json!("booth");
    product["sourceCategory"] = json!("Clothes");
    product["sourceTags"] = json!(["authored-clothing", "authored-compatible"]);
    f.service
        .catalogue
        .put("product", "42", product.clone())
        .unwrap();
    let retained = f.artifact(0);
    let library = f.path.join("library");
    let retained_path =
        io::safe_join(&library, retained["relativePath"].as_str().unwrap()).unwrap();
    let partial = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(partial["status"], "partial");
    assert_eq!(partial["downloaded"], 1);
    assert_eq!(partial["total"], 3);
    assert_eq!(partial["manifestComplete"], false);

    let bytes_a = authored_zip(&[
        ("衣装/empty/", b""),
        ("衣装/model.unitypackage", b"authored package bytes"),
        ("衣装/nested.zip", b"nested bytes remain unexpanded"),
    ]);
    let bytes_b = authored_zip(&[("manual/readme.txt", b"authored manual bytes")]);
    let (mut archive_a, original_a) = f.publish_authored_download(1, "outfit.zip", &bytes_a);
    let (mut archive_b, original_b) = f.publish_authored_download(2, "manual.zip", &bytes_b);
    let user_sibling = original_b.parent().unwrap().join("user-note.txt");
    std::fs::write(&user_sibling, b"authored user retained sibling").unwrap();
    let unrelated = library.join("unrelated-retained.bin");
    std::fs::write(&unrelated, b"authored unrelated retained bytes").unwrap();
    assert!(!library.join(".nna-partials").exists());
    let root = f.service.root(Some("test-root")).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    for artifact in [&mut archive_a, &mut archive_b] {
        let files = f.service.remote_files("42").unwrap().0;
        f.service
            .extract_for_download(&tool, &root, artifact, &files, &cancel, &Default::default())
            .await
            .unwrap();
        assert_eq!(artifact["status"], "ready");
        assert_eq!(artifact["originalAvailable"], false);
        assert!(f
            .service
            .artifact_verified(artifact, &library)
            .await
            .unwrap());
    }
    let uncertified = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(uncertified["status"], "ready");
    assert_eq!(uncertified["downloaded"], 3);
    assert_eq!(uncertified["manifestComplete"], false);
    assert_eq!(uncertified["fullyDownloaded"], true);
    assert!(uncertified["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| file["status"] == "ready"));
    // Source coverage stays incomplete, while the observed files are usable.
    let mut entitlement = f
        .service
        .catalogue
        .get("entitlement", "test-entitlement")
        .unwrap()
        .unwrap();
    entitlement["manifestComplete"] = json!(true);
    f.service
        .catalogue
        .put("entitlement", "test-entitlement", entitlement.clone())
        .unwrap();
    let ready = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["downloaded"], 3);
    assert_eq!(ready["total"], 3);
    assert_eq!(ready["manifestComplete"], true);
    assert_eq!(ready["fullyDownloaded"], true);
    assert!(!original_a.exists());
    assert!(!original_b.exists());
    assert!(!original_a.parent().unwrap().parent().unwrap().exists());
    assert_eq!(
        std::fs::read(&user_sibling).unwrap(),
        b"authored user retained sibling"
    );
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"authored unrelated retained bytes"
    );
    assert!(!library.join(".nna-staging").exists());
    assert!(!library.join(".nna-partials").exists());
    assert!(f
        .service
        .catalogue
        .list("artifact")
        .unwrap()
        .iter()
        .all(|artifact| !artifact["id"].as_str().unwrap().starts_with("pending-")));
    let extracted_a = f
        .service
        .verify_extracted_at(&archive_a, &library, &AtomicBool::new(false))
        .unwrap();
    assert!(extracted_a.join("衣装/empty").is_dir());
    assert_eq!(
        std::fs::read(extracted_a.join("衣装/model.unitypackage")).unwrap(),
        b"authored package bytes"
    );
    assert_eq!(
        std::fs::read(extracted_a.join("衣装/nested.zip")).unwrap(),
        b"nested bytes remain unexpanded"
    );
    assert!(!extracted_a.join("衣装/nested").exists());
    for index in [1, 2] {
        let file = f
            .service
            .catalogue
            .get("remote_file", &format!("test-file-{index}"))
            .unwrap()
            .unwrap();
        assert_eq!(file["requiresContentVerification"], false);
        assert_eq!(
            file["sourceObservationToken"],
            format!("authored-source-{index}")
        );
    }
    assert_eq!(
        f.service.catalogue.get("product", "42").unwrap().unwrap(),
        product
    );
    io::verify(
        &retained_path,
        retained["hash"].as_str().unwrap(),
        retained["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    let Fixture {
        service,
        path: fixture_path,
    } = f;
    drop(service);

    let catalogue = Catalogue::open(&fixture_path.join("data")).unwrap();
    assert_eq!(
        catalogue
            .command("product_details", json!({"id":"42"}))
            .unwrap()["status"],
        "ready"
    );
    assert_eq!(catalogue.get("product", "42").unwrap().unwrap(), product);
    assert_eq!(
        catalogue
            .get("entitlement", "test-entitlement")
            .unwrap()
            .unwrap(),
        entitlement
    );
    let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
    let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
    let reopened = StorageService::new(catalogue.clone(), stores).unwrap();
    let durable = catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(durable["status"], "ready");
    assert_eq!(durable["fullyDownloaded"], true);
    assert_eq!(durable["downloaded"], 3);
    assert_eq!(
        std::fs::read(&user_sibling).unwrap(),
        b"authored user retained sibling"
    );
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"authored unrelated retained bytes"
    );
    drop(reopened);
    drop(catalogue);
    assert!(fixture_path.is_absolute());
    assert_eq!(fixture_path.parent().unwrap(), std::env::temp_dir());
    assert!(fixture_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("nna-storage-synthetic-"));
    archive::validate_no_links(&fixture_path).unwrap();
    std::fs::remove_dir_all(fixture_path).unwrap();
}

#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
async fn real_extraction_failure_retains_original_and_retry_cancel_are_distinct_from_partial() {
    let tool = native_seven_zip_test_tool();
    let f = Fixture::new();
    f.seed(2, true);
    let sibling = f.artifact(1);
    let library = f.path.join("library");
    let (mut failed, corrupt_original) =
        f.publish_authored_download(0, "broken.zip", b"authored corrupt zip bytes");
    let corrupt_state = io::hash_file(&corrupt_original).unwrap();
    let root = f.service.root(Some("test-root")).unwrap();
    let files = f.service.remote_files("42").unwrap().0;
    let error = f
        .service
        .extract_for_download(
            &tool,
            &root,
            &mut failed,
            &files,
            &Arc::new(AtomicBool::new(false)),
            &Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(io::stable_error(&error), "archive_corrupt");
    assert_eq!(failed["status"], "extractionFailed");
    assert_eq!(failed["originalAvailable"], true);
    assert_eq!(failed["errorCode"], "archive_corrupt");
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", failed["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        failed
    );
    assert_eq!(io::hash_file(&corrupt_original).unwrap(), corrupt_state);
    assert!(!library.join(".nna-staging").exists());
    assert_eq!(
        f.service
            .catalogue
            .command("product_details", json!({"id":"42"}))
            .unwrap()["status"],
        "attention"
    );
    let retry = f
        .service
        .enqueue_file(&json!({"fileId":"test-file-0"}))
        .await
        .unwrap();
    let mut retry_job = f
        .service
        .catalogue
        .get("job", retry["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(retry_job["selectedFileIds"], json!(["test-file-0"]));
    assert_eq!(
        f.service
            .catalogue
            .command("product_details", json!({"id":"42"}))
            .unwrap()["status"],
        "processing"
    );
    let good_bytes = authored_zip(&[("repaired/model.unitypackage", b"authored repaired package")]);
    let (mut repaired, retry_original) = f.publish_authored_download(0, "broken.zip", &good_bytes);
    let files = f.service.remote_files("42").unwrap().0;
    let cancelled = f
        .service
        .extract_for_download(
            &tool,
            &root,
            &mut repaired,
            &files,
            &Arc::new(AtomicBool::new(true)),
            &Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(io::stable_error(&cancelled), "cancelled");
    assert_eq!(repaired["originalAvailable"], true);
    assert!(retry_original.is_file());
    f.service
        .put_job(&mut retry_job, "cancelled", None)
        .unwrap();
    let partial = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(partial["status"], "partial");
    assert!(partial["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["id"] == "test-file-0" && file["status"] == "downloaded"));
    let final_retry = f
        .service
        .enqueue_file(&json!({"fileId":"test-file-0"}))
        .await
        .unwrap();
    assert_ne!(final_retry, retry);
    let mut final_job = f
        .service
        .catalogue
        .get("job", final_retry["jobIds"][0].as_str().unwrap())
        .unwrap()
        .unwrap();
    let files = f.service.remote_files("42").unwrap().0;
    f.service
        .extract_for_download(
            &tool,
            &root,
            &mut repaired,
            &files,
            &Arc::new(AtomicBool::new(false)),
            &Default::default(),
        )
        .await
        .unwrap();
    f.service
        .put_job(&mut final_job, "completed", None)
        .unwrap();
    assert_eq!(repaired["status"], "ready");
    assert_eq!(repaired["errorCode"], Value::Null);
    assert_eq!(repaired["originalAvailable"], false);
    assert!(!retry_original.exists());
    assert!(!retry_original.parent().unwrap().parent().unwrap().exists());
    assert_eq!(io::hash_file(&corrupt_original).unwrap(), corrupt_state);
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", sibling["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        sibling
    );
    assert!(f
        .service
        .artifact_verified(&sibling, &library)
        .await
        .unwrap());
    assert!(!library.join(".nna-staging").exists());
    assert!(!library.join(".nna-partials").exists());
    let ready = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["manifestComplete"], true);
    assert_eq!(ready["fullyDownloaded"], true);
    let extracted = f
        .service
        .verify_extracted_at(&repaired, &library, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        std::fs::read(extracted.join("repaired/model.unitypackage")).unwrap(),
        b"authored repaired package"
    );
}

#[test]
fn archive_deletion_crash_reconciles_only_with_verified_extracted_inventory() {
    let f = Fixture::new();
    f.seed(1, true);
    let mut a = f.artifact(0);
    let path = f.path.join("library/extracted/body.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"verified extraction").unwrap();
    let (hash, size) = io::hash_file(&path).unwrap();
    a["extractedPath"] = json!("extracted");
    a["extractedInventory"] = json!([{"relativePath":"body.txt","hash":hash,"sizeBytes":size}]);
    a["originalDeletionPending"] = json!(true);
    f.service
        .catalogue
        .put("artifact", a["id"].as_str().unwrap(), a.clone())
        .unwrap();
    std::fs::remove_file(
        f.path
            .join("library/assets")
            .join(io::readable_component("Synthetic clothing", "42"))
            .join("fixture-0/fixture-0.unitypackage"),
    )
    .unwrap();
    f.service.reconcile_archive_cleanup().unwrap();
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", a["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["originalAvailable"],
        false
    );
    assert!(path.exists());
}

#[test]
fn empty_extraction_inventory_cannot_restore_ready_or_replace_catalogue() {
    let f = Fixture::new();
    f.seed(1, true);
    let a = f.artifact(0);
    let mut manifest = f.service.catalogue.manifest().unwrap();
    let artifact = &mut manifest["records"]["artifact"][0];
    artifact["originalAvailable"] = json!(false);
    artifact["extractedPath"] = json!("nonexistent");
    artifact["extractedInventory"] = json!([]);
    let backup = f.path.join("invalid-empty.zip");
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
}

#[test]
fn product_availability_uses_actual_original_and_extracted_paths() {
    let f = Fixture::new();
    f.seed(1, true);
    let mut a = f.artifact(0);
    let args = json!({"productId":"42"});
    assert_eq!(
        f.service.product_availability(&args).unwrap()["openableFileIds"],
        json!(["test-file-0"])
    );
    a["originalAvailable"] = json!(false);
    f.service
        .catalogue
        .put("artifact", a["id"].as_str().unwrap(), a.clone())
        .unwrap();
    let unavailable = f.service.product_availability(&args).unwrap();
    assert_eq!(unavailable["folderAvailable"], false);
    assert_eq!(unavailable["openableFileIds"], json!([]));
}

#[test]
fn restart_discards_only_owned_partial_and_preserves_completed_artifact() {
    let f = Fixture::new();
    f.seed(2, true);
    let ready = f.artifact(0);
    let root = f.path.join("library");
    std::fs::create_dir_all(root.join(".nna-partials")).unwrap();
    std::fs::write(root.join(".nna-partials/pending-test.part"), b"partial").unwrap();
    std::fs::write(root.join("keep.bin"), b"unrelated original").unwrap();
    f.service.catalogue.put("artifact","pending-test",json!({"id":"pending-test","productId":"42","fileId":"test-file-1","rootId":"test-root","relativePath":"assets/incomplete.bin","status":"downloading","originalAvailable":false})).unwrap();
    f.service.reconcile_archive_cleanup().unwrap();
    assert!(!root.join(".nna-partials/pending-test.part").exists());
    assert!(!root.join(".nna-partials").exists());
    assert!(root.join("keep.bin").exists());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", ready["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        ready
    );
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", "pending-test")
            .unwrap()
            .unwrap()["status"],
        "notDownloaded"
    );
    // Its download job resumes at this start, so the interrupted file is no fault.
    let product = f
        .service
        .catalogue
        .command("product_details", json!({"id":"42"}))
        .unwrap();
    let file = product["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["id"] == "test-file-1")
        .unwrap();
    assert_eq!(
        (&product["status"], &file["status"]),
        (&json!("partial"), &json!("notDownloaded"))
    );
}

#[tokio::test]
async fn an_archive_kept_beside_its_ready_extraction_is_removed_like_an_extracted_one() {
    let f = Fixture::new();
    f.seed(2, true);
    let root = f
        .service
        .catalogue
        .get("root", "test-root")
        .unwrap()
        .unwrap();
    let library = f.path.join("library");
    let (artifact, original, extracted) = f.extracted_archive(0, true);
    f.service
        .remove_kept_original(&root, &library, &artifact)
        .await;
    assert!(!original.exists());
    assert!(extracted.join("model.fbx").is_file());
    let record = f
        .service
        .catalogue
        .get("artifact", artifact["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(record["originalAvailable"], false);
    assert_eq!(record["status"], "ready");
    assert!(record.get("originalDeletionPending").is_none());
    // Another record naming the same bytes that is not Ready keeps them.
    let (second, original, _) = f.extracted_archive(1, true);
    f.service.catalogue.put("artifact","historical-1",json!({"id":"historical-1","productId":"42","fileId":"test-file-1","rootId":"test-root","relativePath":second["relativePath"],"status":"downloaded","originalAvailable":true})).unwrap();
    f.service
        .remove_kept_original(&root, &library, &second)
        .await;
    assert!(original.is_file());
}
