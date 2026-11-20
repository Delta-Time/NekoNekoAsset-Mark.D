
use super::*;

#[cfg(windows)]
#[tokio::test]
async fn selected_source_sibling_reuses_actual_extraction_after_archive_original_deletion() {
    use std::io::Write;
    let root = std::env::temp_dir().join(format!(
        "nna-extracted-source-sibling-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    {
        let catalogue = Catalogue::open(&root.join("data")).unwrap();
        let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
        let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
        let service = StorageService::new(catalogue.clone(), stores).unwrap();
        let mapped_root = json!({"id":"authored-root","path":root,"status":"online"});
        catalogue
            .put("root", "authored-root", mapped_root.clone())
            .unwrap();
        catalogue.put("product", "jinxxy:authored", json!({"id":"jinxxy:authored","title":"Authored files","provider":"jinxxy","sourceCategory":{"name":"Tool"}})).unwrap();
        let archive = json!({"id":"authored-archive","provider":"jinxxy","productId":"jinxxy:authored","accountId":"authored-account","inventoryId":"authored-inventory","name":"Textures.zip","sourceRelativePath":"Textures.zip","revision":"source:r1"});
        let mut leaf = archive.clone();
        leaf["id"] = json!("authored-leaf");
        leaf["name"] = json!("Model.fbx");
        leaf["sourceRelativePath"] = json!("Model.fbx");
        let files = [archive.clone(), leaf.clone()];
        for file in &files {
            catalogue
                .put("remote_file", file["id"].as_str().unwrap(), file.clone())
                .unwrap();
        }
        let directory = "assets/Authored--stable";
        let original_relative = source_tree_relative(&archive, "download:first", directory)
            .unwrap()
            .unwrap();
        let original = io::safe_join(&root, &original_relative).unwrap();
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&original).unwrap());
        zip.start_file(
            "textures/authored.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"authored texture payload").unwrap();
        zip.finish().unwrap();
        let (hash, size) = io::hash_file(&original).unwrap();
        let mut artifact = json!({"id":artifact_id(&archive),"fileId":archive["id"],"productId":archive["productId"],"rootId":"authored-root","relativePath":original_relative,"productDirectory":directory,"revision":archive["revision"],"hash":hash,"sizeBytes":size,"status":"downloaded","originalAvailable":true});
        catalogue
            .put(
                "artifact",
                artifact["id"].as_str().unwrap(),
                artifact.clone(),
            )
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let tool = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tools/7zip/7z.exe");
        service
            .extract_one(&tool, &mapped_root, &mut artifact, &files, &cancel)
            .await
            .unwrap();
        assert!(!original.exists());
        assert_eq!(artifact["originalAvailable"], false);
        let stable_extraction = artifact["extractedPath"].clone();
        let stable_inventory = artifact["extractedInventory"].clone();
        let (updates, expected) = service
            .prepare_selected_source_siblings(
                &mapped_root,
                &root,
                &leaf,
                &files,
                "download:second",
                directory,
                &cancel,
            )
            .await
            .unwrap();
        assert!(updates.is_empty());
        assert_eq!(expected.len(), 2);
        assert_eq!(
            catalogue
                .get("artifact", &artifact_id(&archive))
                .unwrap()
                .unwrap()["extractedPath"],
            stable_extraction
        );
        assert_eq!(artifact["extractedInventory"], stable_inventory);
        let relative = source_tree_relative(&leaf, "download:second", directory)
            .unwrap()
            .unwrap();
        let path = io::safe_join(&root, &relative).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"authored model payload").unwrap();
        let (hash, size) = io::hash_file(&path).unwrap();
        catalogue.put("artifact", &artifact_id(&leaf), json!({"id":artifact_id(&leaf),"fileId":leaf["id"],"productId":leaf["productId"],"rootId":"authored-root","relativePath":relative,"productDirectory":directory,"revision":leaf["revision"],"hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true})).unwrap();
        assert!(service
            .verified_source_manifest(&mapped_root, &root, &files, true, &cancel)
            .await
            .unwrap());
        assert!(!original.exists());
        let entry = artifact["extractedInventory"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["isDirectory"] != true)
            .unwrap();
        let extracted = io::safe_join(&root, stable_extraction.as_str().unwrap()).unwrap();
        std::fs::write(
            io::safe_join(&extracted, entry["relativePath"].as_str().unwrap()).unwrap(),
            b"changed authored bytes",
        )
        .unwrap();
        assert_eq!(
            service
                .prepare_selected_source_siblings(
                    &mapped_root,
                    &root,
                    &leaf,
                    &files,
                    "download:third",
                    directory,
                    &cancel,
                )
                .await
                .unwrap_err()
                .to_string(),
            "source_tree_incomplete"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_transient_resolution_errors_retry_and_actionable_failures_remain_terminal() {
    for code in [
        "NETWORK_UNAVAILABLE",
        "network_failed",
        "network_timeout",
        "DOWNLOAD_RESOLVER_FAILED",
        "SIGNED_LINK_EXPIRED",
    ] {
        assert!(
            retryable_download_resolution(&anyhow::anyhow!(code)),
            "{code}"
        );
    }
    for code in [
        "AUTH_REQUIRED",
        "RATE_LIMITED",
        "PERMISSION_DENIED",
        "FILE_UNAVAILABLE",
        "SOURCE_CHANGED",
        "UNSAFE_DOWNLOAD_URL",
        "cancelled",
        "low_space",
        "invalid_resolver",
    ] {
        assert!(
            !retryable_download_resolution(&anyhow::anyhow!(code)),
            "{code}"
        );
    }
}

#[test]
fn download_retry_after_respects_dates_and_never_shortens_long_server_deferrals() {
    let now = chrono::DateTime::parse_from_rfc2822("Mon, 12 Oct 2026 01:00:00 GMT")
        .unwrap()
        .timestamp();
    assert_eq!(
        download_retry_delay(Some("120"), 0, now),
        Some(Duration::from_secs(120))
    );
    assert_eq!(download_retry_delay(Some("121"), 0, now), None);
    assert_eq!(
        download_retry_delay(Some("0"), 0, now),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        download_retry_delay(None, 2, now),
        Some(Duration::from_secs(4))
    );
    assert_eq!(
        download_retry_delay(Some("Mon, 12 Oct 2026 01:00:10 GMT"), 0, now),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        download_retry_delay(Some("Mon, 12 Oct 2026 01:03:00 GMT"), 0, now),
        None
    );
}

#[test]
fn source_tree_publication_reuses_staged_bytes_and_preserves_existing_targets() {
    let root = std::env::temp_dir().join(format!(
        "nna-source-publication-synthetic-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join(".nna-partials")).unwrap();
    let part = root.join(".nna-partials/file.part");
    let relative = "source/file.bin";
    let target = root.join(relative);
    let bytes = b"synthetic source bytes";
    std::fs::write(&part, bytes).unwrap();
    let (hash, size) = io::hash_file(&part).unwrap();
    let cancel = AtomicBool::new(false);
    #[cfg(windows)]
    let mut original = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&part)
        .unwrap();
    publish_source_tree_part(&root, relative, &part, &hash, size, &cancel).unwrap();
    assert!(!part.exists());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    #[cfg(windows)]
    {
        use std::io::Write;
        // The open staged handle still addresses the final file, proving
        // publication moved the file object instead of copying its bytes.
        original.set_len(0).unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 0);
        original.write_all(bytes).unwrap();
        original.sync_all().unwrap();
        drop(original);
    }
    // A matching retry removes only its own partial and keeps the target.
    std::fs::write(&part, bytes).unwrap();
    publish_source_tree_part(&root, relative, &part, &hash, size, &cancel).unwrap();
    assert!(!part.exists());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);

    std::fs::write(&part, bytes).unwrap();
    std::fs::write(&target, b"preserve existing target").unwrap();
    assert!(publish_source_tree_part(&root, relative, &part, &hash, size, &cancel).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"preserve existing target");
    assert_eq!(std::fs::read(&part).unwrap(), bytes);

    cancel.store(true, Ordering::Release);
    assert!(
        publish_source_tree_part(&root, "cancelled/file.bin", &part, &hash, size, &cancel).is_err()
    );
    assert!(!root.join("cancelled").exists());
    assert_eq!(std::fs::read(&part).unwrap(), bytes);
    std::fs::remove_file(&part).unwrap();
    std::fs::remove_file(&target).unwrap();
    std::fs::remove_dir(root.join(".nna-partials")).unwrap();
    std::fs::remove_dir(root.join("source")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[tokio::test]
async fn source_file_tree_preserves_sibling_paths_verified_reuse_and_prior_bytes() {
    let root = std::env::temp_dir().join(format!(
        "nna-source-tree-synthetic-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join("old-flat")).unwrap();
    let model = json!({"id":"model","provider":"jinxxy","productId":"jinxxy:synthetic","accountId":"synthetic-account","inventoryId":"synthetic-inventory","name":"model.fbx","sourceRelativePath":"folder/model.fbx","revision":"source:r1"});
    let mut material = model.clone();
    material["id"] = json!("material");
    material["name"] = json!("material.mat");
    material["sourceRelativePath"] = json!("folder/materials/material.mat");
    let mut other = material.clone();
    other["id"] = json!("other-material");
    other["sourceRelativePath"] = json!("alternate/material.mat");
    validate_source_tree_paths(&[model.clone(), material.clone(), other.clone()]).unwrap();
    let model_relative = source_tree_relative(&model, "download:one", "Tool")
        .unwrap()
        .unwrap();
    let material_relative = source_tree_relative(&material, "download:one", "Tool")
        .unwrap()
        .unwrap();
    assert_eq!(
        source_tree_relative(&model, "download:one", "Tool")
            .unwrap()
            .unwrap(),
        model_relative
    );
    std::fs::write(
        root.join("old-flat/model.fbx"),
        b"references materials/material.mat",
    )
    .unwrap();
    std::fs::write(
        root.join("old-flat/material.mat"),
        b"synthetic material bytes",
    )
    .unwrap();
    let (model_hash, model_size) = io::hash_file(&root.join("old-flat/model.fbx")).unwrap();
    let (material_hash, material_size) =
        io::hash_file(&root.join("old-flat/material.mat")).unwrap();
    let cancel = AtomicBool::new(false);
    retain_source_tree_copy(
        &root,
        "old-flat/model.fbx",
        &model_relative,
        &model_hash,
        model_size,
        &cancel,
    )
    .unwrap();
    retain_source_tree_copy(
        &root,
        "old-flat/material.mat",
        &material_relative,
        &material_hash,
        material_size,
        &cancel,
    )
    .unwrap();
    let model_path = io::safe_join(&root, &model_relative).unwrap();
    let material_path = model_path.parent().unwrap().join("materials/material.mat");
    assert_eq!(
        material_path,
        io::safe_join(&root, &material_relative).unwrap()
    );
    io::verify(&material_path, &material_hash, material_size).unwrap();
    {
        let catalogue = Catalogue::open(&root.join("data")).unwrap();
        let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
        let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
        let service = StorageService::new(catalogue.clone(), stores).unwrap();
        let mapped_root = json!({"id":"synthetic-root","path":root,"status":"online"});
        catalogue
            .put("root", "synthetic-root", mapped_root.clone())
            .unwrap();
        let model_artifact = json!({"id":artifact_id(&model),"fileId":model["id"],"productId":model["productId"],"rootId":"synthetic-root","relativePath":model_relative,"revision":model["revision"],"hash":model_hash,"sizeBytes":model_size,"status":"ready","originalAvailable":true});
        let material_artifact = json!({"id":artifact_id(&material),"fileId":material["id"],"productId":material["productId"],"rootId":"synthetic-root","relativePath":material_relative,"revision":material["revision"],"hash":material_hash,"sizeBytes":material_size,"status":"ready","originalAvailable":true});
        for artifact in [&model_artifact, &material_artifact] {
            catalogue
                .put(
                    "artifact",
                    artifact["id"].as_str().unwrap(),
                    artifact.clone(),
                )
                .unwrap();
        }
        let files = [model.clone(), material.clone()];
        let before = catalogue.list("artifact").unwrap();
        let mut selected_files = files.to_vec();
        selected_files.push(other.clone()); // Unselected, undownloaded leaves stay partial.
        let (prepared, expected) = service
            .prepare_selected_source_siblings(
                &mapped_root,
                &root,
                &model,
                &selected_files,
                "download:selected",
                "assets/Synthetic--stable",
                &Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        assert_eq!(prepared.len(), 2); // One retained historical + one current copied sibling.
        assert_eq!(expected.len(), 2);
        let copied = prepared
            .iter()
            .find(|(_, id, _)| id == &artifact_id(&material))
            .unwrap();
        let copied_path = io::safe_join(&root, copied.2["relativePath"].as_str().unwrap()).unwrap();
        io::verify(&copied_path, &material_hash, material_size).unwrap();
        assert_eq!(catalogue.list("artifact").unwrap(), before);
        io::verify(
            &io::safe_join(&root, &material_relative).unwrap(),
            &material_hash,
            material_size,
        )
        .unwrap();
        let mut unavailable = material_artifact.clone();
        unavailable["originalAvailable"] = json!(false);
        catalogue
            .put("artifact", &artifact_id(&material), unavailable)
            .unwrap();
        assert_eq!(
            service
                .prepare_selected_source_siblings(
                    &mapped_root,
                    &root,
                    &model,
                    &selected_files,
                    "download:selected-missing",
                    "assets/Synthetic--stable",
                    &Arc::new(AtomicBool::new(false)),
                )
                .await
                .unwrap_err()
                .to_string(),
            "source_tree_incomplete"
        );
        catalogue
            .put(
                "artifact",
                &artifact_id(&material),
                material_artifact.clone(),
            )
            .unwrap();
        // Repeated selected updates keep the same verified tree and artifact records.
        for _ in 0..2 {
            assert!(service
                .verified_source_manifest(&mapped_root, &root, &files, true, &cancel)
                .await
                .unwrap());
        }
        assert_eq!(catalogue.list("artifact").unwrap(), before);
        // A source without an updated timestamp still performs GET; only
        // its actual equal hash/size may retain this single-leaf Ready path.
        let mut unverified = model.clone();
        unverified["requiresContentVerification"] = json!(true);
        assert_eq!(
            service
                .verified_single_source_content(
                    &mapped_root,
                    &root,
                    &model,
                    &[unverified.clone()],
                    true,
                    &model_hash,
                    model_size
                )
                .await
                .unwrap(),
            Some(model_artifact.clone())
        );
        assert!(service
            .verified_single_source_content(
                &mapped_root,
                &root,
                &model,
                &[unverified.clone()],
                true,
                "different-actual-hash",
                model_size
            )
            .await
            .unwrap()
            .is_none());
        assert!(service
            .verified_single_source_content(
                &mapped_root,
                &root,
                &model,
                &[unverified.clone()],
                true,
                &model_hash,
                model_size + 1
            )
            .await
            .unwrap()
            .is_none());
        let mut moved = model.clone();
        moved["sourceRelativePath"] = json!("moved/model.fbx");
        assert!(service
            .verified_single_source_content(
                &mapped_root,
                &root,
                &moved,
                &[moved.clone()],
                true,
                &model_hash,
                model_size
            )
            .await
            .unwrap()
            .is_none());
        assert!(service
            .verified_single_source_content(
                &mapped_root,
                &root,
                &model,
                &[unverified, material.clone()],
                true,
                &model_hash,
                model_size
            )
            .await
            .unwrap()
            .is_none());
        assert_eq!(catalogue.list("artifact").unwrap(), before);
        assert!(!io::safe_join(
            &root,
            &source_tree_relative(&model, "download:two", "Tool")
                .unwrap()
                .unwrap()
        )
        .unwrap()
        .exists());
        let mut changed = model.clone();
        changed["revision"] = json!("source:r2");
        assert!(!service
            .verified_source_manifest(
                &mapped_root,
                &root,
                &[changed, material.clone()],
                true,
                &cancel
            )
            .await
            .unwrap());
        assert!(!service
            .verified_source_manifest(
                &mapped_root,
                &root,
                &[model.clone(), material.clone(), other.clone()],
                true,
                &cancel
            )
            .await
            .unwrap());
        // An interrupted mixed layout must still rebuild one coherent sibling tree.
        let mixed_relative = source_tree_relative(&material, "download:interrupted", "Tool")
            .unwrap()
            .unwrap();
        retain_source_tree_copy(
            &root,
            &material_relative,
            &mixed_relative,
            &material_hash,
            material_size,
            &cancel,
        )
        .unwrap();
        let mut mixed_artifact = material_artifact.clone();
        mixed_artifact["relativePath"] = json!(mixed_relative);
        catalogue
            .put("artifact", &artifact_id(&material), mixed_artifact)
            .unwrap();
        assert!(!service
            .verified_source_manifest(&mapped_root, &root, &files, true, &cancel)
            .await
            .unwrap());
        catalogue
            .put("artifact", &artifact_id(&material), material_artifact)
            .unwrap();
        std::fs::remove_file(&model_path).unwrap();
        assert!(service
            .verified_single_source_content(
                &mapped_root,
                &root,
                &model,
                &[model.clone()],
                true,
                &model_hash,
                model_size
            )
            .await
            .unwrap()
            .is_none());
        assert!(!service
            .verified_source_manifest(&mapped_root, &root, &files, true, &cancel)
            .await
            .unwrap());
        retain_source_tree_copy(
            &root,
            "old-flat/model.fbx",
            &model_relative,
            &model_hash,
            model_size,
            &cancel,
        )
        .unwrap();
        assert!(service
            .verified_source_manifest(&mapped_root, &root, &files, true, &cancel)
            .await
            .unwrap());
        assert_eq!(catalogue.list("artifact").unwrap(), before);
    }
    // Same-job retry reuses complete bytes, while a new job retains a separate tree.
    retain_source_tree_copy(
        &root,
        "old-flat/model.fbx",
        &model_relative,
        &model_hash,
        model_size,
        &cancel,
    )
    .unwrap();
    let next_relative = source_tree_relative(&model, "download:two", "Tool")
        .unwrap()
        .unwrap();
    assert_ne!(next_relative, model_relative);
    retain_source_tree_copy(
        &root,
        &model_relative,
        &next_relative,
        &model_hash,
        model_size,
        &cancel,
    )
    .unwrap();
    io::verify(&root.join("old-flat/model.fbx"), &model_hash, model_size).unwrap();
    io::verify(&model_path, &model_hash, model_size).unwrap();
    // A changed destination is preserved rather than replaced by matching old bytes.
    std::fs::write(&material_path, b"retained damaged destination").unwrap();
    assert!(retain_source_tree_copy(
        &root,
        "old-flat/material.mat",
        &material_relative,
        &material_hash,
        material_size,
        &cancel
    )
    .is_err());
    assert_eq!(
        std::fs::read(&material_path).unwrap(),
        b"retained damaged destination"
    );
    io::verify(
        &root.join("old-flat/material.mat"),
        &material_hash,
        material_size,
    )
    .unwrap();
    let mut unsafe_file = model.clone();
    unsafe_file["sourceRelativePath"] = json!("../model.fbx");
    assert!(validate_source_tree_paths(&[unsafe_file]).is_err());
    let mut collision = model.clone();
    collision["id"] = json!("another");
    collision["sourceRelativePath"] = json!("Folder/model.fbx");
    assert!(validate_source_tree_paths(&[model.clone(), collision]).is_err());
    let mut moved = model.clone();
    moved["sourceRelativePath"] = json!("other/model.fbx");
    assert!(!same_remote_identity(&model, &moved));
    let mut booth = model;
    booth["provider"] = json!("booth");
    assert!(source_tree_relative(&booth, "download:one", "Tool")
        .unwrap()
        .is_none());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn source_tree_maps_names_windows_cannot_store_and_still_refuses_traversal() {
    let file = json!({"id":"synthetic","provider":"jinxxy","productId":"jinxxy:synthetic","accountId":"synthetic-account","inventoryId":"synthetic-inventory","name":"Read me?.txt","sourceRelativePath":"Docs <v2>/Read me?.txt","revision":"source:r1"});
    let relative = source_tree_relative(&file, "download:one", "Tool")
        .unwrap()
        .unwrap();
    assert!(
        relative.ends_with("/Docs ＜v2＞/Read me？.txt"),
        "{relative}"
    );
    archive::validate_relative_path(&relative).unwrap();
    // Re-downloads land at the same path, and the job no longer fails on the name.
    assert_eq!(
        source_tree_relative(&file, "download:one", "Tool")
            .unwrap()
            .unwrap(),
        relative
    );
    validate_source_tree_paths(std::slice::from_ref(&file)).unwrap();
    for unsafe_path in ["../Read me?.txt", "Docs//Read me?.txt", "./Read me?.txt"] {
        let mut unsafe_file = file.clone();
        unsafe_file["sourceRelativePath"] = json!(unsafe_path);
        assert!(
            source_tree_relative(&unsafe_file, "download:one", "Tool").is_err(),
            "{unsafe_path}"
        );
    }
}

#[test]
fn verified_provider_file_length_cannot_be_overridden_by_response_body_length() {
    use crate::stores::StoreProvider;
    for provider in [StoreProvider::Gumroad, StoreProvider::Jinxxy] {
        let error = expected_download_size(provider, Some(12), Some(42), None).unwrap_err();
        assert_eq!(io::stable_error(&error), "file_integrity_failed");
        assert_eq!(
            expected_download_size(provider, Some(0), Some(0), Some(42)).unwrap(),
            Some(0)
        );
        assert_eq!(
            expected_download_size(provider, None, Some(42), Some(12)).unwrap(),
            Some(42)
        );
        assert_eq!(
            expected_download_size(provider, Some(42), None, None).unwrap(),
            Some(42)
        );
    }
    assert_eq!(
        expected_download_size(StoreProvider::Booth, Some(12), Some(42), None).unwrap(),
        Some(12)
    );
    assert_eq!(
        expected_download_size(StoreProvider::Booth, None, Some(42), Some(12)).unwrap(),
        Some(42)
    );
    assert_eq!(
        expected_download_size(StoreProvider::Booth, None, None, Some(12)).unwrap(),
        None
    );
}
#[test]
fn final_revision_uses_positive_source_or_actual_verified_content() {
    let unknown = json!({"revision":"unknown"});
    assert_eq!(
        bind_revision(&unknown, &unknown, None, "abcd").unwrap(),
        ("sha256:abcd".into(), "contentHash".into())
    );
    assert_eq!(
        bind_revision(&unknown, &unknown, Some("etag:actual".into()), "abcd")
            .unwrap()
            .0,
        "etag:actual"
    );
    let source = json!({"revision":"source:v1","revisionBasis":"source"});
    assert_eq!(
        bind_revision(&source, &source, Some("etag:actual".into()), "abcd")
            .unwrap()
            .0,
        "source:v1"
    );
}
#[test]
fn source_hash_mismatch_fails_but_historical_hash_does_not_override_new_content() {
    let expected = json!({"revision":"sha256:expected","revisionBasis":"source"});
    assert!(bind_revision(&expected, &expected, None, "different").is_err());
    let historical = json!({"revision":"sha256:old","revisionBasis":"source","requiresContentVerification":true});
    assert_eq!(
        bind_revision(&historical, &historical, None, "new")
            .unwrap()
            .0,
        "sha256:new"
    );
}
