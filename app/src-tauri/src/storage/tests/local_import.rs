//! Local file, folder and ZIP import.
use super::*;

#[test]
fn local_import_single_preserves_names_empty_directories_originals_and_truthful_metadata() {
    let f = Fixture::new();
    let first = f.path.join("source-a/衣装");
    let second = f.path.join("source-b/衣装");
    std::fs::create_dir_all(first.join("nested/empty")).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(
        first.join("nested/model.unitypackage"),
        b"first synthetic payload",
    )
    .unwrap();
    std::fs::write(second.join("notes.txt"), b"second synthetic payload").unwrap();
    let thumbnail = f.path.join("data/media/local-thumbnail.png");
    std::fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
    std::fs::write(&thumbnail, b"independently authored thumbnail payload").unwrap();
    let accepted = f.service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4().to_string(),"mode":"single","sources":[first,second],"category":"Tool","name":"Synthetic grouped tool","thumbnailPath":"media/local-thumbnail.png"})).unwrap();
    let mut job = f
        .service
        .catalogue
        .get("job", accepted["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    f.service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    let id = job["productIds"][0].as_str().unwrap();
    let product = f.service.catalogue.get("product", id).unwrap().unwrap();
    assert!(id.starts_with("local:"));
    assert_eq!(product["provider"], "local");
    assert_eq!(product["category"], "Tool");
    assert_eq!(
        f.service.catalogue.get("overlay", id).unwrap().unwrap()["category"],
        "Tool"
    );
    let effective = f
        .service
        .catalogue
        .command("product_details", json!({"id":id}))
        .unwrap();
    assert_eq!(effective["category"], "Tool");
    assert_eq!(
        f.service.catalogue.get("overlay", id).unwrap().unwrap()["thumbnailPath"],
        "media/local-thumbnail.png"
    );
    assert_eq!(
        PathBuf::from(effective["previewUrl"].as_str().unwrap()),
        thumbnail
    );
    let visible = f
        .service
        .catalogue
        .command("query", json!({"category":"Tool"}))
        .unwrap();
    assert!(visible["matchedIds"]
        .as_array()
        .unwrap()
        .contains(&json!(id)));
    assert_eq!(
        PathBuf::from(
            visible["products"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == id)
                .unwrap()["previewUrl"]
                .as_str()
                .unwrap()
        ),
        thumbnail
    );
    assert!(product["price"].is_null());
    assert!(chrono::NaiveDateTime::parse_from_str(
        product["acquired"].as_str().unwrap(),
        "%Y/%m/%d %H:%M"
    )
    .is_ok());
    assert!(product["importedAt"].is_string());
    assert!(!f.path.join("library/.nna-import-partials").exists());
    assert_eq!(product["localManifest"]["complete"], true);
    assert_eq!(
        product["localManifest"]["fileIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(strings(&product["localManifest"]["directories"])
        .iter()
        .any(|d| d.ends_with("衣装/nested/empty")));
    let (files, complete) = f.service.remote_files(id).unwrap();
    assert!(complete);
    assert!(files.iter().all(|v| v["originKind"] == "local"
        && v["accountId"].is_null()
        && v["sourceUrl"].is_null()));
    for artifact in f.service.catalogue.related("artifact", id).unwrap() {
        assert_eq!(artifact["status"], "ready");
        assert_eq!(artifact["originalAvailable"], true);
        io::verify(
            &io::safe_join(
                &f.path.join("library"),
                artifact["relativePath"].as_str().unwrap(),
            )
            .unwrap(),
            artifact["hash"].as_str().unwrap(),
            artifact["sizeBytes"].as_u64().unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        std::fs::read(first.join("nested/model.unitypackage")).unwrap(),
        b"first synthetic payload"
    );
    assert_eq!(
        std::fs::read(second.join("notes.txt")).unwrap(),
        b"second synthetic payload"
    );
    assert_eq!(job["completedProductIds"], job["productIds"]);
    let manifest = serde_json::to_string(&f.service.catalogue.manifest().unwrap()).unwrap();
    assert!(!manifest.contains(&first.to_string_lossy().replace('\\', "\\\\")));
    assert!(f.service.catalogue.list("checkpoint").unwrap().is_empty());
    let backup = f.path.join("local-full.nna-backup");
    f.service
        .disk_operation(
            "export_backup",
            &json!({"path":backup}),
            &AtomicBool::new(false),
        )
        .unwrap();
    let restored = portable::restore_backup(
        &backup,
        &f.path.join("restored-copy"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(restored
        .payload
        .iter()
        .any(|e| e.is_directory && e.relative_path.ends_with("衣装/nested/empty")));
    let restored_thumbnail = restored
        .payload
        .iter()
        .find(|e| e.relative_path == "media/local-thumbnail.png")
        .unwrap();
    assert_eq!(
        std::fs::read(
            f.path
                .join("restored-copy")
                .join(&restored_thumbnail.archive_path)
        )
        .unwrap(),
        b"independently authored thumbnail payload"
    );
}

#[test]
fn local_import_duplicate_reuses_product_and_draft_submission_is_idempotent() {
    let f = Fixture::new();
    let source = f.path.join("original.unitypackage");
    std::fs::write(&source, b"synthetic repeated content").unwrap();
    let draft = uuid::Uuid::new_v4().to_string();
    let args = json!({"draftId":draft,"sources":[source],"category":"Clothes"});
    let first = f.service.prepare_local_import(&args).unwrap();
    assert_eq!(
        first,
        f.service
            .prepare_local_import(&args)
            .unwrap()
            .as_object()
            .map(|v| json!({"jobId":v["jobId"],"productIds":v["productIds"]}))
            .unwrap()
    );
    let mut first_job = f
        .service
        .catalogue
        .get("job", first["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    f.service
        .run_local_import(
            &mut first_job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    let first_id = first_job["productIds"][0].as_str().unwrap();
    f.service
        .catalogue
        .command(
            "overlay",
            json!({"ids":[first_id],"patch":{"category":"Tool","note":"Retain human metadata","thumbnailPath":"media/human-thumbnail.png"}}),
        )
        .unwrap();
    let mut second_job = local_job(&f, &[source.clone()], "single");
    second_job["localPlan"]["metadata"]["thumbnailPath"] = json!("media/duplicate-thumbnail.png");
    f.service
        .run_local_import(
            &mut second_job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(second_job["productIds"], first_job["productIds"]);
    assert_eq!(
        second_job["completedProductIds"],
        first_job["completedProductIds"]
    );
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
    assert_eq!(f.service.catalogue.list("artifact").unwrap().len(), 1);
    assert_eq!(f.service.catalogue.list("acquisition").unwrap().len(), 1);
    let effective = f
        .service
        .catalogue
        .command("product_details", json!({"id":first_id}))
        .unwrap();
    assert_eq!(effective["category"], "Tool");
    assert_eq!(effective["note"], "Retain human metadata");
    assert_eq!(
        f.service
            .catalogue
            .get("overlay", first_id)
            .unwrap()
            .unwrap()["thumbnailPath"],
        "media/human-thumbnail.png"
    );
    assert!(source.exists());
}

#[test]
fn local_import_restart_uses_verified_copy_when_completed_source_has_gone() {
    let f = Fixture::new();
    let first = f.path.join("a.unitypackage");
    let second = f.path.join("b.txt");
    let held = f.path.join("b-held.txt");
    std::fs::write(&first, b"completed synthetic content").unwrap();
    std::fs::write(&second, b"unfinished synthetic content").unwrap();
    let mut job = local_job(&f, &[first.clone(), second.clone()], "single");
    std::fs::rename(&second, &held).unwrap();
    assert!(f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false)
        )
        .is_err());
    assert_eq!(f.service.catalogue.list("checkpoint").unwrap().len(), 1);
    assert!(f.service.catalogue.list("product").unwrap().is_empty());
    let completed_copy = f.service.catalogue.list("checkpoint").unwrap()[0].clone();
    let cancel = AtomicBool::new(true);
    assert_eq!(
        io::stable_error(
            &f.service
                .run_local_import(&mut job, Path::new("missing-synthetic-sevenzip"), &cancel)
                .unwrap_err()
        ),
        "cancelled"
    );
    io::verify(
        &io::safe_join(
            &f.path.join("library"),
            completed_copy["relativePath"].as_str().unwrap(),
        )
        .unwrap(),
        completed_copy["hash"].as_str().unwrap(),
        completed_copy["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    assert!(first.exists() && held.exists());
    let remapped = f.path.join("remapped-library");
    std::fs::create_dir_all(&remapped).unwrap();
    let mut root = f
        .service
        .catalogue
        .get("root", "test-root")
        .unwrap()
        .unwrap();
    root["path"] = json!(remapped);
    f.service.catalogue.put("root", "test-root", root).unwrap();
    // The first copy's checkpoint survives remapping, but bytes are still at
    // the old root. Retry must recover that missing copy from unchanged input.
    assert!(f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false)
        )
        .is_err());
    let remapped_copy =
        io::safe_join(&remapped, completed_copy["relativePath"].as_str().unwrap()).unwrap();
    io::verify(
        &remapped_copy,
        completed_copy["hash"].as_str().unwrap(),
        completed_copy["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    assert!(first.exists() && held.exists());
    let old_copy = io::safe_join(
        &f.path.join("library"),
        completed_copy["relativePath"].as_str().unwrap(),
    )
    .unwrap();
    io::verify(
        &old_copy,
        completed_copy["hash"].as_str().unwrap(),
        completed_copy["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    std::fs::write(&remapped_copy, b"synthetic damaged retained bytes").unwrap();
    assert!(f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false)
        )
        .is_err());
    assert_eq!(
        std::fs::read(&remapped_copy).unwrap(),
        b"synthetic damaged retained bytes"
    );
    assert_eq!(
        std::fs::read(&first).unwrap(),
        b"completed synthetic content"
    );
    std::fs::copy(&old_copy, &remapped_copy).unwrap();
    std::fs::remove_file(&first).unwrap();
    std::fs::rename(&held, &second).unwrap();
    f.service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(f.service.catalogue.list("artifact").unwrap().len(), 2);
    assert!(f.service.catalogue.list("checkpoint").unwrap().is_empty());
}

#[test]
fn local_import_batch_failure_keeps_prior_group_completed_and_managed_archive_original() {
    let f = Fixture::new();
    let folder = f.path.join("a-folder");
    let archive = f.path.join("b-corrupt.zip");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("model.unitypackage"), b"first complete product").unwrap();
    std::fs::write(&archive, b"synthetic invalid zip").unwrap();
    let mut job = local_job(&f, &[folder, archive.clone()], "batch");
    let error = f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(io::stable_error(&error), "archive_tool_unavailable");
    assert_eq!(job["completedProductIds"], json!([job["productIds"][0]]));
    assert_eq!(job["progress"], 50.0);
    let first_artifact = &f
        .service
        .catalogue
        .related("artifact", job["productIds"][0].as_str().unwrap())
        .unwrap()[0];
    assert_eq!(first_artifact["status"], "ready");
    let second_artifact = &f
        .service
        .catalogue
        .related("artifact", job["productIds"][1].as_str().unwrap())
        .unwrap()[0];
    assert_eq!(second_artifact["status"], "extractionFailed");
    assert_eq!(second_artifact["originalAvailable"], true);
    assert_eq!(std::fs::read(&archive).unwrap(), b"synthetic invalid zip");
    f.service
        .put_job(&mut job, "needsAction", Some("archive_tool_unavailable"))
        .unwrap();
    let attention = f
        .service
        .catalogue
        .command("query", json!({"quick":"attention","includeFiles":false}))
        .unwrap();
    assert_eq!(attention["matched"], 1);
    assert_eq!(attention["products"][0]["id"], job["productIds"][1]);
    io::verify(
        &io::safe_join(
            &f.path.join("library"),
            second_artifact["relativePath"].as_str().unwrap(),
        )
        .unwrap(),
        second_artifact["hash"].as_str().unwrap(),
        second_artifact["sizeBytes"].as_u64().unwrap(),
    )
    .unwrap();
    // Retry must re-run failed extraction rather than mistaking its own marker for a duplicate.
    assert!(f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false)
        )
        .is_err());
    assert_eq!(job["completedProductIds"].as_array().unwrap().len(), 1);
}

#[test]
fn local_import_batch_failure_does_not_block_later_groups_or_recheck_completed_membership() {
    let f = Fixture::new();
    let archive = f.path.join("a-corrupt.zip");
    let folder_b = f.path.join("b-folder");
    let folder_c = f.path.join("c-folder");
    std::fs::write(&archive, b"authored invalid archive").unwrap();
    for (folder, bytes) in [(&folder_b, b"independent B"), (&folder_c, b"independent C")] {
        std::fs::create_dir(folder).unwrap();
        std::fs::write(folder.join("model.unitypackage"), bytes).unwrap();
    }
    let mut job = local_job(&f, &[archive, folder_b, folder_c], "batch");
    let run = |job: &mut Value| {
        f.service.run_local_import(
            job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
    };
    assert_eq!(
        io::stable_error(&run(&mut job).unwrap_err()),
        "archive_tool_unavailable"
    );
    assert_eq!(job["completedGroupIndexes"], json!([1, 2]));
    assert_eq!(job["completedProductIds"].as_array().unwrap().len(), 2);
    assert_eq!(job["result"]["failedGroups"].as_array().unwrap().len(), 1);
    for index in [1, 2] {
        let id = job["productIds"][index].as_str().unwrap();
        let mut product = f.service.catalogue.get("product", id).unwrap().unwrap();
        product["localManifest"]["lastMutationId"] = json!("later-user-membership-change");
        f.service
            .catalogue
            .put("product", id, product.clone())
            .unwrap();
        assert_eq!(
            f.service.catalogue.related("artifact", id).unwrap()[0]["status"],
            "ready"
        );
    }
    assert_eq!(
        io::stable_error(&run(&mut job).unwrap_err()),
        "archive_tool_unavailable"
    );
    assert_eq!(job["result"]["failedGroups"].as_array().unwrap().len(), 1);
    assert_eq!(job["completedProductIds"].as_array().unwrap().len(), 2);
}

#[cfg(windows)]
#[test]
fn local_import_copies_a_name_windows_reserves_under_the_name_7zip_gives_it() {
    let f = Fixture::new();
    let folder = f.path.join("source").join("素材");
    std::fs::create_dir_all(&folder).unwrap();
    // A verbatim path stores aux.png as a file on any Windows; Windows 11 needs none.
    let folder = folder.canonicalize().unwrap();
    std::fs::write(folder.join("aux.png"), b"device-named texture").unwrap();
    std::fs::write(folder.join("normal.png"), b"ordinary texture").unwrap();
    let mut job = local_job(&f, std::slice::from_ref(&folder), "single");
    f.service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    let product = job["productIds"][0].as_str().unwrap();
    let mut copies: Vec<_> = f
        .service
        .catalogue
        .related("artifact", product)
        .unwrap()
        .iter()
        .map(|artifact| {
            let relative = artifact["relativePath"].as_str().unwrap().to_owned();
            let bytes = std::fs::read(io::safe_join(&f.path.join("library"), &relative).unwrap());
            (
                relative.rsplit('/').next().unwrap().to_owned(),
                bytes.unwrap(),
            )
        })
        .collect();
    copies.sort();
    assert_eq!(
        copies,
        [
            ("_aux.png".to_owned(), b"device-named texture".to_vec()),
            ("normal.png".to_owned(), b"ordinary texture".to_vec())
        ]
    );
    assert_eq!(
        std::fs::read(folder.join("aux.png")).unwrap(),
        b"device-named texture"
    );
    std::fs::remove_file(folder.join("aux.png")).unwrap();
}

#[test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
fn local_import_preserves_leading_space_names_in_an_archive_with_trailing_data() {
    let f = Fixture::new();
    let source = f.path.join("authored-leading-space.zip");
    let mut bytes = authored_zip(&[
        (" leading.txt", b"literal space"),
        ("nested/ inner.txt", b"nested space"),
    ]);
    bytes.extend_from_slice(b"authored harmless trailing data");
    std::fs::write(&source, &bytes).unwrap();
    let mut job = local_job(&f, &[source.clone()], "single");
    f.service
        .run_local_import(
            &mut job,
            &native_seven_zip_test_tool(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let artifact = &f
        .service
        .catalogue
        .related("artifact", job["productIds"][0].as_str().unwrap())
        .unwrap()[0];
    assert_eq!(artifact["status"], "ready");
    let extracted = io::safe_join(
        &f.path.join("library"),
        artifact["extractedPath"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(extracted.join(" leading.txt")).unwrap(),
        b"literal space"
    );
    assert_eq!(
        std::fs::read(extracted.join("nested/ inner.txt")).unwrap(),
        b"nested space"
    );
    assert_eq!(std::fs::read(source).unwrap(), bytes);
}

#[test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
fn local_import_retry_reuses_only_recorded_unchanged_complete_extraction() {
    let tool = native_seven_zip_test_tool();
    let f = Fixture::new();
    let source = f.path.join("a-complete.zip");
    let failed = f.path.join("b-corrupt.zip");
    std::fs::write(
        &source,
        authored_zip(&[("model.unitypackage", b"authored reusable package")]),
    )
    .unwrap();
    std::fs::write(&failed, b"authored invalid archive").unwrap();
    let original = io::hash_file(&source).unwrap();
    let mut job = local_job(&f, &[source.clone(), failed.clone()], "single");
    assert!(f
        .service
        .run_local_import(&mut job, &tool, &AtomicBool::new(false))
        .is_err());
    let artifacts = f.service.catalogue.list("artifact").unwrap();
    let completed = artifacts
        .iter()
        .find(|a| {
            a["relativePath"]
                .as_str()
                .unwrap()
                .ends_with("a-complete.zip")
        })
        .unwrap()
        .clone();
    assert_eq!(completed["status"], "ready");
    assert_eq!(
        artifacts
            .iter()
            .filter(|a| a["status"] == "extractionFailed")
            .count(),
        1
    );
    let library = f.path.join("library");
    let extracted = io::safe_join(&library, completed["extractedPath"].as_str().unwrap()).unwrap();
    let member = extracted.join("model.unitypackage");
    let missing_tool = f.path.join("missing-synthetic-sevenzip");
    // The sibling still fails, but this known successful ZIP must stay Ready
    // even when invoking the extractor is impossible: no new output was written.
    let error = f
        .service
        .run_local_import(&mut job, &missing_tool, &AtomicBool::new(false))
        .unwrap_err();
    assert_eq!(io::stable_error(&error), "archive_tool_unavailable");
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", completed["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        completed
    );
    assert_eq!(
        std::fs::read(&member).unwrap(),
        b"authored reusable package"
    );
    assert!(!library.join(".nna-import-staging").exists());

    // A dropped old volume must prevent reuse even if every currently supplied
    // member still matches the saved bytes and inventory.
    let mut old_volume = completed.clone();
    old_volume["id"] = json!("authored-dropped-volume");
    old_volume["fileId"] = json!("authored-dropped-file");
    let old_relative = Path::new(completed["relativePath"].as_str().unwrap())
        .with_file_name("a-complete.z01")
        .to_string_lossy()
        .replace('\\', "/");
    old_volume["relativePath"] = json!(old_relative);
    f.service
        .catalogue
        .put("artifact", "authored-dropped-volume", old_volume)
        .unwrap();
    assert!(f
        .service
        .run_local_import(&mut job, &missing_tool, &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", completed["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["status"],
        "extractionFailed"
    );
    f.service
        .catalogue
        .delete("artifact", "authored-dropped-volume")
        .unwrap();

    // An existing output directory alone is not proof of successful publication.
    let mut no_inventory = completed.clone();
    no_inventory
        .as_object_mut()
        .unwrap()
        .remove("extractedInventory");
    f.service
        .catalogue
        .put("artifact", completed["id"].as_str().unwrap(), no_inventory)
        .unwrap();
    assert!(f
        .service
        .run_local_import(&mut job, &missing_tool, &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", completed["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["status"],
        "extractionFailed"
    );

    // User-modified output still takes the existing full-extraction fallback,
    // leaving that tree intact when extraction cannot run.
    f.service
        .catalogue
        .put(
            "artifact",
            completed["id"].as_str().unwrap(),
            completed.clone(),
        )
        .unwrap();
    std::fs::write(&member, b"authored retained user edit").unwrap();
    assert!(f
        .service
        .run_local_import(&mut job, &missing_tool, &AtomicBool::new(false))
        .is_err());
    assert_eq!(
        f.service
            .catalogue
            .get("artifact", completed["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["status"],
        "extractionFailed"
    );
    assert_eq!(
        std::fs::read(&member).unwrap(),
        b"authored retained user edit"
    );
    assert_eq!(io::hash_file(&source).unwrap(), original);
    io::verify(
        &io::safe_join(&library, completed["relativePath"].as_str().unwrap()).unwrap(),
        &original.0,
        original.1,
    )
    .unwrap();
    assert_eq!(std::fs::read(&failed).unwrap(), b"authored invalid archive");
}

#[test]
fn local_import_rejected_publication_does_not_skip_group_on_retry() {
    let f = Fixture::new();
    let first = f.path.join("first-folder");
    let second = f.path.join("second-folder");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let first_input = first.join("model.unitypackage");
    let second_input = second.join("model.unitypackage");
    let held = f.path.join("held-second.unitypackage");
    std::fs::write(&first_input, b"prior completed synthetic group").unwrap();
    std::fs::write(&second_input, b"retry unpublished synthetic group").unwrap();
    let mut job = local_job(&f, &[first, second], "batch");
    std::fs::rename(&second_input, &held).unwrap();
    assert!(f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false)
        )
        .is_err());
    let completed = job["completedProductIds"].clone();
    let completed_indexes = job["completedGroupIndexes"].clone();
    assert_eq!(completed_indexes, json!([0]));
    let first_id = job["productIds"][0].as_str().unwrap().to_owned();
    let prior_product = f
        .service
        .catalogue
        .get("product", &first_id)
        .unwrap()
        .unwrap();
    std::fs::rename(&held, &second_input).unwrap();
    // Force a deterministic catalogue publication rejection after copying the
    // next group, without a filesystem failpoint or a new test framework.
    job["localPlan"]["metadata"]["category"] = json!("invalid synthetic category");
    let error = f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(error.to_string().contains("catalogue.invalid_category"));
    assert_eq!(job["completedProductIds"], completed);
    assert_eq!(job["completedGroupIndexes"], completed_indexes);
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
    assert!(f
        .service
        .catalogue
        .get("product", job["productIds"][1].as_str().unwrap())
        .unwrap()
        .is_none());
    assert_eq!(f.service.catalogue.list("checkpoint").unwrap().len(), 1);
    // Match process_local_job's terminal write and reopen its durable job.
    f.service
        .put_job(&mut job, "needsAction", Some("storage_failed"))
        .unwrap();
    let mut retry = f
        .service
        .catalogue
        .get("job", job["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(retry["completedGroupIndexes"], json!([0]));
    retry["localPlan"]["metadata"]["category"] = json!("Clothes");
    f.service
        .run_local_import(
            &mut retry,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(retry["completedProductIds"], retry["productIds"]);
    assert_eq!(retry["completedGroupIndexes"], json!([0, 1]));
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 2);
    assert_eq!(
        f.service
            .catalogue
            .get("product", &first_id)
            .unwrap()
            .unwrap(),
        prior_product
    );
    assert!(f.service.catalogue.list("checkpoint").unwrap().is_empty());
    assert_eq!(
        std::fs::read(first_input).unwrap(),
        b"prior completed synthetic group"
    );
    assert_eq!(
        std::fs::read(second_input).unwrap(),
        b"retry unpublished synthetic group"
    );
}

#[test]
fn local_import_changed_source_refuses_publication_and_conflicting_draft_resubmission() {
    let f = Fixture::new();
    let source = f.path.join("changed.unitypackage");
    std::fs::write(&source, b"planned synthetic bytes").unwrap();
    let args =
        json!({"draftId":uuid::Uuid::new_v4().to_string(),"sources":[source],"category":"Base"});
    let result = f.service.prepare_local_import(&args).unwrap();
    let mut changed = args.clone();
    changed["name"] = json!("Changed form for an accepted draft");
    assert!(f
        .service
        .prepare_local_import(&changed)
        .unwrap_err()
        .to_string()
        .contains("local_draft_changed"));
    let mut job = f
        .service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    std::fs::write(&source, b"different length synthetic payload").unwrap();
    let error = f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(error.to_string().contains("local_source_changed"));
    assert!(f.service.catalogue.list("product").unwrap().is_empty());
    assert!(f.service.catalogue.list("artifact").unwrap().is_empty());
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"different length synthetic payload"
    );
}

#[test]
#[cfg_attr(not(windows), ignore = "requires explicit NNA_7ZIP on non-Windows")]
fn local_import_real_7zip_preserves_managed_zip_nested_archive_and_unitypackage() {
    use std::io::Write;
    let tool = native_seven_zip_test_tool();
    let f = Fixture::new();
    let source = f.path.join("local-source.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    writer.add_directory("衣装/empty/", options).unwrap();
    writer
        .start_file("衣装/model.unitypackage", options)
        .unwrap();
    writer
        .write_all(b"synthetic unity package retained")
        .unwrap();
    writer.start_file("衣装/nested.zip", options).unwrap();
    writer
        .write_all(b"synthetic nested zip remains unexpanded")
        .unwrap();
    writer.finish().unwrap();
    let original = io::hash_file(&source).unwrap();
    let mut job = local_job(&f, &[source.clone()], "single");
    f.service
        .run_local_import(&mut job, &tool, &AtomicBool::new(false))
        .unwrap();
    let artifact = &f.service.catalogue.list("artifact").unwrap()[0];
    assert_eq!(artifact["status"], "ready");
    assert_eq!(artifact["originalAvailable"], true);
    assert_eq!(io::hash_file(&source).unwrap(), original);
    io::verify(
        &io::safe_join(
            &f.path.join("library"),
            artifact["relativePath"].as_str().unwrap(),
        )
        .unwrap(),
        &original.0,
        original.1,
    )
    .unwrap();
    let extracted = f
        .service
        .verify_extracted_at(artifact, &f.path.join("library"), &AtomicBool::new(false))
        .unwrap();
    assert!(extracted.join("衣装/empty").is_dir());
    assert_eq!(
        std::fs::read(extracted.join("衣装/model.unitypackage")).unwrap(),
        b"synthetic unity package retained"
    );
    assert_eq!(
        std::fs::read(extracted.join("衣装/nested.zip")).unwrap(),
        b"synthetic nested zip remains unexpanded"
    );
    assert!(!extracted.join("衣装/nested").exists());

    // A missing member marks the file missing; Retry must publish a new complete
    // tree without replacing the damaged tree or either archive copy.
    std::fs::remove_file(extracted.join("衣装/model.unitypackage")).unwrap();
    std::fs::write(
        extracted.join("user-retained.txt"),
        b"synthetic retained edit",
    )
    .unwrap();
    f.service.put_job(&mut job, "completed", None).unwrap();
    let mut missing = artifact.clone();
    missing["previousStatus"] = missing["status"].clone();
    missing["status"] = json!("missing");
    missing["errorCode"] = json!("file_integrity_failed");
    f.service
        .catalogue
        .put("artifact", artifact["id"].as_str().unwrap(), missing)
        .unwrap();
    let product = job["productIds"][0].as_str().unwrap().to_owned();
    let job_id = job["id"].as_str().unwrap().to_owned();
    assert_eq!(
        f.service
            .retry_local_product_locked(&product, &f.service.catalogue.list("job").unwrap())
            .unwrap(),
        Some(job_id.clone())
    );
    job = f.service.catalogue.get("job", &job_id).unwrap().unwrap();
    f.service
        .run_local_import(&mut job, &tool, &AtomicBool::new(false))
        .unwrap();
    let repaired = f.service.catalogue.list("artifact").unwrap()[0].clone();
    assert_eq!(repaired["status"], "ready");
    assert_eq!(repaired["originalAvailable"], true);
    assert_ne!(repaired["extractedPath"], artifact["extractedPath"]);
    let repaired_path = f
        .service
        .verify_extracted_at(&repaired, &f.path.join("library"), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        std::fs::read(repaired_path.join("衣装/model.unitypackage")).unwrap(),
        b"synthetic unity package retained"
    );
    assert!(repaired_path.join("衣装/empty").is_dir());
    assert!(!repaired_path.join("衣装/nested").exists());
    assert_eq!(
        std::fs::read(extracted.join("user-retained.txt")).unwrap(),
        b"synthetic retained edit"
    );
    assert!(!extracted.join("衣装/model.unitypackage").exists());
    assert_eq!(io::hash_file(&source).unwrap(), original);
    io::verify(
        &io::safe_join(
            &f.path.join("library"),
            repaired["relativePath"].as_str().unwrap(),
        )
        .unwrap(),
        &original.0,
        original.1,
    )
    .unwrap();
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
    assert_eq!(job["completedProductIds"], json!([product]));

    // Retrying a group with a recorded usable repair must reuse that tree,
    // even though its original extraction destination remains damaged.
    let mut retry_artifact = repaired.clone();
    retry_artifact["status"] = json!("extractionFailed");
    f.service
        .catalogue
        .put("artifact", repaired["id"].as_str().unwrap(), retry_artifact)
        .unwrap();
    f.service.put_job(&mut job, "completed", None).unwrap();
    f.service
        .retry_local_product_locked(&product, &f.service.catalogue.list("job").unwrap())
        .unwrap();
    job = f.service.catalogue.get("job", &job_id).unwrap().unwrap();
    f.service
        .run_local_import(&mut job, &tool, &AtomicBool::new(false))
        .unwrap();
    let repeated = f.service.catalogue.list("artifact").unwrap()[0].clone();
    assert_eq!(repeated["status"], "ready");
    assert_eq!(repeated["extractedPath"], repaired["extractedPath"]);
    assert_eq!(
        std::fs::read(extracted.join("user-retained.txt")).unwrap(),
        b"synthetic retained edit"
    );
}

#[tokio::test]
async fn local_import_retry_clears_cancellation_without_creating_another_job() {
    let f = Fixture::new();
    let source = f.path.join("cancelled.unitypackage");
    std::fs::write(&source, b"synthetic cancelled import").unwrap();
    let mut job = local_job(&f, &[source], "single");
    let id = job["id"].as_str().unwrap().to_owned();
    job["cancelRequested"] = json!(true);
    job["attentionDismissed"] = json!(["authored"]);
    f.service.put_job(&mut job, "cancelled", None).unwrap();
    f.service.cancellation(&id).store(true, Ordering::Release);
    // Clear history only takes an import that has not added its asset out of Activity,
    // once; Storage keeps it for this Retry.
    for cleared in [1, 0] {
        let result = f
            .service
            .catalogue
            .command("clear_activity", json!({}))
            .unwrap();
        assert_eq!(result["cleared"], cleared);
    }
    let kept = f.service.catalogue.get("job", &id).unwrap().unwrap();
    assert_eq!(kept["activityCleared"], true);
    let response = f.service.retry_local_import(&id).await.unwrap();
    assert_eq!(response["jobId"], id);
    let mut resumed = f.service.catalogue.get("job", &id).unwrap().unwrap();
    assert_eq!(resumed["state"], "queued");
    assert_eq!(resumed["cancelRequested"], false);
    assert!(resumed.get("attentionDismissed").is_none());
    assert!(resumed.get("activityCleared").is_none());
    assert!(!f.service.cancellation(&id).load(Ordering::Acquire));
    f.service
        .run_local_import(
            &mut resumed,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 1);
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
}

#[tokio::test]
async fn local_import_discard_removes_only_copies_of_assets_it_did_not_add() {
    let f = Fixture::new();
    let library = f.path.join("library");
    let added = f.path.join("added");
    let stopped = f.path.join("stopped");
    let extra = f.path.join("extra");
    std::fs::create_dir_all(stopped.join("nested/empty")).unwrap();
    for directory in [&added, &extra] {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join("1-copied.txt"), b"synthetic copied payload").unwrap();
    }
    std::fs::write(
        stopped.join("nested/1-copied.txt"),
        b"synthetic copied payload",
    )
    .unwrap();
    std::fs::write(stopped.join("nested/2-changed.txt"), b"synthetic payload").unwrap();
    std::fs::write(extra.join("2-changed.txt"), b"synthetic payload").unwrap();
    // The second asset's source changes after the plan, so the import stops part way.
    let mut job = local_job(&f, &[added, stopped.clone()], "batch");
    let id = job["id"].as_str().unwrap().to_owned();
    std::fs::write(
        stopped.join("nested/2-changed.txt"),
        b"synthetic changed payload",
    )
    .unwrap();
    let error = f
        .service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(error.to_string().contains("local_source_changed"));
    f.service
        .put_job(&mut job, "failed", Some("local_source_changed"))
        .unwrap();
    let product = job["completedProductIds"][0].as_str().unwrap().to_owned();
    let groups = &job["localPlan"]["groups"];
    let never_added = library.join(groups[1]["prefix"].as_str().unwrap());
    assert!(f
        .service
        .catalogue
        .get("product", groups[1]["product_id"].as_str().unwrap())
        .unwrap()
        .is_none());
    assert!(never_added.join("stopped/nested/1-copied.txt").is_file());
    // A crash can leave a partial copy behind.
    let partials = library.join(".nna-import-partials");
    let draft = id.strip_prefix("local-import:").unwrap();
    std::fs::create_dir_all(partials.join(draft)).unwrap();
    std::fs::write(partials.join(draft).join("left.part"), b"synthetic partial").unwrap();
    // Adding files to the asset it added stops part way too.
    let accepted = f
        .service
        .mutate_local_files(&json!({"draftId":uuid::Uuid::new_v4().to_string(),"productId":product,"addSources":[&extra]}))
        .await
        .unwrap();
    let add_id = accepted["jobId"].as_str().unwrap().to_owned();
    let mut add = f.service.catalogue.get("job", &add_id).unwrap().unwrap();
    let queued = f.service.discard_local_import(&add_id).await.unwrap_err();
    assert!(queued.to_string().contains("invalid_job"));
    std::fs::write(extra.join("2-changed.txt"), b"synthetic changed payload").unwrap();
    f.service
        .run_local_import(
            &mut add,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    f.service
        .put_job(&mut add, "failed", Some("local_source_changed"))
        .unwrap();
    let directory = library.join(groups[0]["prefix"].as_str().unwrap());
    assert!(directory.join("extra/1-copied.txt").is_file());
    for discarded in [&id, &add_id] {
        let answer = f.service.discard_local_import(discarded).await.unwrap();
        assert_eq!(answer["jobId"], *discarded);
        assert!(f.service.catalogue.get("job", discarded).unwrap().is_none());
    }
    // The asset it added keeps every file; the rest of what the imports copied is gone.
    let artifacts = f.service.catalogue.related("artifact", &product).unwrap();
    assert_eq!(artifacts.len(), 1);
    for artifact in &artifacts {
        assert!(library
            .join(artifact["relativePath"].as_str().unwrap())
            .is_file());
    }
    assert!(!directory.join("extra").exists());
    assert!(!never_added.exists());
    assert!(!partials.exists());
    assert!(f.service.catalogue.list("checkpoint").unwrap().is_empty());
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
    // Originals stay.
    assert!(stopped.join("nested/1-copied.txt").is_file());
    assert!(extra.join("1-copied.txt").is_file());
}

#[tokio::test]
async fn local_import_download_routes_to_original_job_repairs_missing_copy_and_never_networks() {
    let f = Fixture::new();
    let first = f.path.join("a-kept.unitypackage");
    let second = f.path.join("b-repaired.txt");
    std::fs::write(&first, b"positive managed copy survives").unwrap();
    std::fs::write(&second, b"original available for repair").unwrap();
    let mut job = local_job(&f, &[first.clone(), second], "single");
    f.service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    f.service.put_job(&mut job, "completed", None).unwrap();
    let product = job["productIds"][0].as_str().unwrap().to_owned();
    let artifacts = f.service.catalogue.related("artifact", &product).unwrap();
    let missing = artifacts
        .iter()
        .find(|a| {
            a["relativePath"]
                .as_str()
                .unwrap()
                .ends_with("b-repaired.txt")
        })
        .unwrap();
    std::fs::remove_file(&first).unwrap();
    std::fs::remove_file(
        io::safe_join(
            &f.path.join("library"),
            missing["relativePath"].as_str().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let response = f
        .service
        .enqueue(&json!({"productIds":[product]}))
        .await
        .unwrap();
    assert_eq!(response["jobIds"], json!([job["id"]]));
    let mut resumed = f
        .service
        .catalogue
        .get("job", job["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(resumed["completedProductIds"], json!([]));
    f.service
        .run_local_import(
            &mut resumed,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    f.service.put_job(&mut resumed, "completed", None).unwrap();
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":[product]}))
            .await
            .unwrap()["jobIds"],
        json!([])
    );
    assert_eq!(f.service.catalogue.list("job").unwrap().len(), 1);
    assert!(f
        .service
        .catalogue
        .list("download_intent")
        .unwrap()
        .is_empty());
    // Clear history removes the import job; the product's own source record still
    // copies the file again from its original folder, in a job of its own.
    f.service
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    let missing = f
        .service
        .catalogue
        .get("artifact", missing["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let managed = io::safe_join(
        &f.path.join("library"),
        missing["relativePath"].as_str().unwrap(),
    )
    .unwrap();
    std::fs::remove_file(&managed).unwrap();
    let before = f
        .service
        .catalogue
        .get("product", &product)
        .unwrap()
        .unwrap();
    let response = f
        .service
        .enqueue(&json!({"productIds":[product]}))
        .await
        .unwrap();
    let repair_id = response["jobIds"][0].as_str().unwrap().to_owned();
    assert_ne!(repair_id, job["id"].as_str().unwrap());
    // Asked again while it waits, the same job answers.
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":[product]}))
            .await
            .unwrap()["jobIds"],
        json!([repair_id])
    );
    let mut repair = f.service.catalogue.get("job", &repair_id).unwrap().unwrap();
    f.service
        .run_local_import(
            &mut repair,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    f.service.put_job(&mut repair, "completed", None).unwrap();
    assert_eq!(
        std::fs::read(&managed).unwrap(),
        b"original available for repair"
    );
    let after = f
        .service
        .catalogue
        .get("product", &product)
        .unwrap()
        .unwrap();
    for key in ["acquired", "acquiredAt", "title", "category"] {
        assert_eq!(after[key], before[key]);
    }
    assert_eq!(f.service.catalogue.list("product").unwrap().len(), 1);
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":[product]}))
            .await
            .unwrap()["jobIds"],
        json!([])
    );
    // A restored backup or another device has no record of these sources here; it must
    // ask for them again.
    f.service
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    f.service
        .catalogue
        .delete("local_source", &product)
        .unwrap();
    std::fs::remove_file(&managed).unwrap();
    let error = f
        .service
        .enqueue(&json!({"productIds":[product]}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("local_source_required"));
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    // Asked for with assets that have nothing to fetch, it still says so.
    f.service
        .catalogue
        .put(
            "product",
            "43",
            json!({"id":"43","title":"Synthetic listing without files"}),
        )
        .unwrap();
    let error = f
        .service
        .enqueue(&json!({"productIds":[product, "43"]}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("local_source_required"));
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    // Asked for with other assets, it stays out and the others still queue.
    f.seed(1, true);
    let response = f
        .service
        .enqueue(&json!({"productIds":[product, "42"]}))
        .await
        .unwrap();
    assert_eq!(response["jobIds"].as_array().unwrap().len(), 1);
    assert_eq!(f.service.catalogue.related("job", "42").unwrap().len(), 1);
}

#[test]
fn local_import_rejects_empty_batch_loose_overlap_and_library_ancestor_without_publication() {
    let f = Fixture::new();
    let empty = f.path.join("empty");
    std::fs::create_dir_all(empty.join("nested")).unwrap();
    assert!(f
        .service
        .prepare_local_import(
            &json!({"draftId":uuid::Uuid::new_v4().to_string(),"sources":[empty]})
        )
        .unwrap_err()
        .to_string()
        .contains("no_import_files"));
    let loose = f.path.join("loose.txt");
    std::fs::write(&loose, b"loose synthetic file").unwrap();
    assert!(f
        .service
        .prepare_local_import(
            &json!({"draftId":uuid::Uuid::new_v4().to_string(),"mode":"batch","sources":[loose]})
        )
        .unwrap_err()
        .to_string()
        .contains("local_batch_requires_folder_or_zip"));
    assert!(f
        .service
        .prepare_local_import(
            &json!({"draftId":uuid::Uuid::new_v4().to_string(),"sources":[f.path.as_path()]})
        )
        .unwrap_err()
        .to_string()
        .contains("local_source_contains_library"));
    let nested = f.path.join("tree");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("file.txt"), b"nested file").unwrap();
    assert!(f.service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4().to_string(),"sources":[nested,nested.join("file.txt")]})).unwrap_err().to_string().contains("local_source_overlap"));
    assert!(f.service.catalogue.list("job").unwrap().is_empty());
    assert!(f.service.catalogue.list("product").unwrap().is_empty());
}

#[test]
fn local_batch_assets_keep_the_order_they_were_added_in() {
    let f = Fixture::new();
    let folders: Vec<_> = ["e", "a", "d", "b", "c"]
        .iter()
        .map(|name| {
            let folder = f.path.join(format!("{name}-folder"));
            std::fs::create_dir(&folder).unwrap();
            std::fs::write(
                folder.join("model.unitypackage"),
                format!("authored {name}"),
            )
            .unwrap();
            folder
        })
        .collect();
    let mut job = local_job(&f, &folders, "batch");
    f.service
        .run_local_import(
            &mut job,
            Path::new("missing-synthetic-sevenzip"),
            &AtomicBool::new(false),
        )
        .unwrap();
    let ids: Vec<String> = job["productIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids.len(), 5);
    let details: Vec<Value> = ids
        .iter()
        .map(|id| {
            f.service
                .catalogue
                .command("product_details", json!({"id":id}))
                .unwrap()
        })
        .collect();
    // One minute shows for the whole import, while each asset sorts a millisecond later.
    assert!(details
        .iter()
        .all(|p| p["acquired"] == details[0]["acquired"]));
    assert!(details
        .windows(2)
        .all(|pair| pair[0]["acquiredSortKey"].as_str() < pair[1]["acquiredSortKey"].as_str()));
    let order = |direction: &str| -> Vec<String> {
        f.service
            .catalogue
            .command(
                "query",
                json!({"sort":"acquired","direction":direction,"hideUnclassified":false}),
            )
            .unwrap()["matchedIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(order("asc"), ids);
    assert_eq!(order("desc"), ids.iter().rev().cloned().collect::<Vec<_>>());
    // A local asset imported before keeps its minute as its key.
    f.service
        .catalogue
        .put(
            "product",
            "older-local",
            json!({"id":"older-local","provider":"local","store":"Local Files","custom":true,"title":"Older","sourceTitle":"Older","acquired":"2026/01/02 03:04"}),
        )
        .unwrap();
    assert_eq!(
        f.service
            .catalogue
            .command("product_details", json!({"id":"older-local"}))
            .unwrap()["acquiredSortKey"],
        "2026-01-02T03:04:00.000000000Z"
    );
}
