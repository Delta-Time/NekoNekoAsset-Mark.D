
use super::*;

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) {
    use std::os::windows::process::CommandExt;
    let link = link
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(link.file_name().unwrap());
    let target = target.canonicalize().unwrap();
    let status = std::process::Command::new("cmd")
        .arg("/d")
        .arg("/c")
        .raw_arg(format!(
            "mklink /J \"{}\" \"{}\"",
            link.to_string_lossy().trim_start_matches(r"\\?\"),
            target.to_string_lossy().trim_start_matches(r"\\?\")
        ))
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{} {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
}

struct Fixture {
    path: PathBuf,
    service: StorageService,
}
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/verification/offline-delete")
            .join(format!(
                "nna-local-delete-authored-{}",
                uuid::Uuid::new_v4()
            ));
        fs::create_dir_all(&path).unwrap();
        let path = path.canonicalize().unwrap();
        #[cfg(windows)]
        let path = PathBuf::from(path.to_string_lossy().trim_start_matches(r"\\?\"));
        let catalogue = Catalogue::open(&path.join("data")).unwrap();
        let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
        let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
        let service = StorageService::new(catalogue, stores).unwrap();
        fs::create_dir_all(path.join("library")).unwrap();
        service
            .catalogue
            .put(
                "root",
                "root",
                json!({"id":"root","path":path.join("library"),"status":"online"}),
            )
            .unwrap();
        Self { path, service }
    }
    fn import(&self, name: &str) -> (String, PathBuf, PathBuf) {
        let original = self.path.join(format!("{name}.txt"));
        fs::write(&original, format!("Authored original {name}")).unwrap();
        let result = self.service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[original],"name":name,"rootId":"root"})).unwrap();
        let mut job = self
            .service
            .catalogue
            .get("job", result["jobId"].as_str().unwrap())
            .unwrap()
            .unwrap();
        self.service
            .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
            .unwrap();
        self.service.put_job(&mut job, "completed", None).unwrap();
        let id = job["productIds"][0].as_str().unwrap().to_owned();
        let directory = self.service.catalogue.related("artifact", &id).unwrap()[0]
            ["productDirectory"]
            .as_str()
            .unwrap()
            .to_owned();
        (id, self.path.join("library").join(directory), original)
    }
    fn finish(self) {
        let Self { path, service } = self;
        drop(service);
        let canonical = path.canonicalize().unwrap();
        let boundary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/verification/offline-delete")
            .canonicalize()
            .unwrap();
        assert!(contains(&boundary, &canonical));
        assert!(canonical
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nna-local-delete-authored-"));
        remove_checked_tree(&canonical).unwrap();
    }
}

#[tokio::test]
async fn delete_local_products_removes_complete_owned_folders_and_preserves_originals() {
    let f = Fixture::new();
    let (a, dir_a, source_a) = f.import("Alpha");
    let (b, dir_b, source_b) = f.import("Beta");
    let (keep, dir_keep, _) = f.import("Keep");
    fs::create_dir_all(dir_a.join("edited/subfolder")).unwrap();
    fs::write(
        dir_a.join("edited/subfolder/user-added.txt"),
        b"owned user addition",
    )
    .unwrap();
    f.service
        .catalogue
        .put("overlay", &a, json!({"id":a,"note":"Authored note"}))
        .unwrap();
    f.service
        .catalogue
        .put(
            "collection",
            "collection",
            json!({"id":"collection","name":"Authored collection","productIds":[a,b,keep]}),
        )
        .unwrap();
    let result = f
        .service
        .delete_local_products(&json!({"productIds":[a,b]}), None)
        .await
        .unwrap();
    assert_eq!(result["deletedProductIds"], json!([a, b]));
    assert!(!dir_a.exists() && !dir_b.exists() && dir_keep.exists());
    assert_eq!(fs::read(source_a).unwrap(), b"Authored original Alpha");
    assert_eq!(fs::read(source_b).unwrap(), b"Authored original Beta");
    for kind in [
        "product",
        "overlay",
        "acquisition",
        "remote_file",
        "artifact",
    ] {
        assert!(f.service.catalogue.get(kind, &a).unwrap().is_none());
        assert!(f.service.catalogue.related(kind, &a).unwrap().is_empty());
    }
    assert_eq!(
        f.service
            .catalogue
            .get("collection", "collection")
            .unwrap()
            .unwrap()["productIds"],
        json!([keep])
    );
    assert!(f
        .service
        .catalogue
        .list("job")
        .unwrap()
        .iter()
        .filter(|j| j["type"] == "storage:import_local")
        .all(
            |j| !strings(&j["productIds"]).contains(&a) && !strings(&j["productIds"]).contains(&b)
        ));
    f.finish();
}

#[tokio::test]
async fn delete_local_products_rejects_mixed_missing_duplicate_and_invalid_selection_atomically() {
    let f = Fixture::new();
    let (a, dir, original) = f.import("Local");
    f.service
        .catalogue
        .put(
            "product",
            "online",
            json!({"id":"online","provider":"booth","title":"Authored online"}),
        )
        .unwrap();
    for (args, code) in [
        (json!({"productIds":[a,"online"]}), "local_product_required"),
        (json!({"productIds":[a,"missing"]}), "product_missing"),
        (json!({"productIds":[a,a]}), "invalid_arguments"),
        (json!({"productIds":[a,7]}), "invalid_arguments"),
        (json!({"productIds":[]}), "invalid_arguments"),
    ] {
        assert_eq!(
            f.service
                .delete_local_products(&args, None)
                .await
                .unwrap_err()
                .to_string(),
            code
        );
        assert!(dir.exists() && original.exists());
        assert!(f.service.catalogue.get("product", &a).unwrap().is_some());
    }
    assert!(f
        .service
        .catalogue
        .list("job")
        .unwrap()
        .iter()
        .all(|j| j["type"] != JOB_TYPE));
    f.finish();
}

#[tokio::test]
async fn delete_local_products_rejects_unsafe_and_shared_directory_references() {
    let f = Fixture::new();
    let (a, directory, _) = f.import("Owned");
    let (other, _, _) = f.import("Other");
    let artifact = f.service.catalogue.related("artifact", &a).unwrap()[0].clone();
    let artifact_id = artifact["id"].as_str().unwrap();
    for unsafe_directory in ["assets", "media/Owned", "assets/Owned/child"] {
        let mut bad = artifact.clone();
        bad["productDirectory"] = json!(unsafe_directory);
        f.service
            .catalogue
            .put("artifact", artifact_id, bad)
            .unwrap();
        assert_eq!(
            f.service
                .delete_local_products(&json!({"productIds":[a]}), None)
                .await
                .unwrap_err()
                .to_string(),
            "local_delete_unsafe_path"
        );
        assert!(directory.exists());
    }
    f.service
        .catalogue
        .put("artifact", artifact_id, artifact.clone())
        .unwrap();
    let mut shared = artifact.clone();
    shared["id"] = json!("shared");
    shared["productId"] = json!(other);
    f.service
        .catalogue
        .put("artifact", "shared", shared)
        .unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "local_delete_shared_path"
    );
    assert!(directory.exists() && f.service.catalogue.get("product", &a).unwrap().is_some());
    f.finish();
}

#[tokio::test]
async fn delete_local_products_rolls_staging_back_when_catalogue_transaction_fails() {
    let f = Fixture::new();
    let (a, directory, original) = f.import("Rollback");
    let connection =
        rusqlite::Connection::open(f.service.catalogue.data_dir().join("catalogue.sqlite3"))
            .unwrap();
    connection.execute_batch("CREATE TRIGGER authored_delete_failure BEFORE DELETE ON entities WHEN OLD.kind='product' BEGIN SELECT RAISE(ABORT,'fixture deletion failure'); END;").unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "local_delete_retry_required"
    );
    assert!(directory.exists() && original.exists());
    assert!(f.service.catalogue.get("product", &a).unwrap().is_some());
    assert!(!f
        .service
        .catalogue
        .related("artifact", &a)
        .unwrap()
        .is_empty());
    let job = f
        .service
        .catalogue
        .list("job")
        .unwrap()
        .into_iter()
        .find(|j| j["type"] == JOB_TYPE)
        .unwrap();
    assert_eq!(job["localDelete"]["recordsDeleted"], false);
    assert_eq!(job["errorCode"], "storage_failed");
    assert_eq!(job["state"], "needsAction");
    connection
        .execute_batch("DROP TRIGGER authored_delete_failure;")
        .unwrap();
    drop(connection);
    f.service
        .delete_local_products(&json!({}), job["id"].as_str())
        .await
        .unwrap();
    assert!(!directory.exists());
    f.finish();
}

#[tokio::test]
async fn delete_local_products_refuses_active_work_and_queued_jobs() {
    let f = Fixture::new();
    let (a, dir, _) = f.import("Busy");
    let work = f.service.catalogue.begin_catalogue_work().unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "storage_busy"
    );
    drop(work);
    f.service
        .catalogue
        .put(
            "job",
            "busy",
            json!({"id":"busy","type":"unity:import","state":"queued"}),
        )
        .unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "storage_busy"
    );
    assert!(dir.exists());
    f.finish();
}

#[tokio::test]
async fn delete_local_products_keeps_empty_product_folder_ownership_after_activity_clear() {
    let f = Fixture::new();
    let (a, dir, original) = f.import("Empty");
    let mut source = f.service.catalogue.get("product", &a).unwrap().unwrap();
    source["localManifest"]["fileIds"] = json!([]);
    source["localManifest"]["complete"] = json!(false);
    f.service.catalogue.put("product", &a, source).unwrap();
    for artifact in f.service.catalogue.related("artifact", &a).unwrap() {
        f.service
            .catalogue
            .delete("artifact", artifact["id"].as_str().unwrap())
            .unwrap();
    }
    for file in f.service.catalogue.related("remote_file", &a).unwrap() {
        f.service
            .catalogue
            .delete("remote_file", file["id"].as_str().unwrap())
            .unwrap();
    }
    f.service
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    // File membership removal intentionally retained these edited managed bytes.
    fs::write(dir.join("retained-edited.txt"), b"authored retained edit").unwrap();
    f.service
        .delete_local_products(&json!({"productIds":[a]}), None)
        .await
        .unwrap();
    assert!(!dir.exists() && original.exists());
    // Pure metadata local records own no physical directory to guess.
    f.service
        .catalogue
        .put(
            "product",
            "local:metadata",
            json!({"id":"local:metadata","provider":"local","title":"Authored metadata only"}),
        )
        .unwrap();
    f.service
        .delete_local_products(&json!({"productIds":["local:metadata"]}), None)
        .await
        .unwrap();
    f.finish();
}

#[tokio::test]
async fn delete_local_products_restores_interrupted_precommit_staging_before_retry() {
    let f = Fixture::new();
    let (a, dir, original) = f.import("Interrupted");
    let id = uuid::Uuid::new_v4().to_string();
    let directories = f.service.delete_plan(&[a.clone()], &id).unwrap();
    let (_, staged) = f.service.delete_paths(&id, 0, &directories[0]).unwrap();
    let job = json!({"id":id,"type":JOB_TYPE,"originDeviceId":f.service.catalogue.device_id(),"productIds":[a],"state":"needsAction","errorCode":"interrupted","localDelete":{"directories":directories,"recordsDeleted":false}});
    f.service.catalogue.put("job", &id, job).unwrap();
    fs::rename(&dir, &staged).unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "storage_busy"
    );
    assert!(f.service.catalogue.get("product", &a).unwrap().is_some());
    f.service
        .delete_local_products(&json!({}), Some(&id))
        .await
        .unwrap();
    assert!(!dir.exists() && !staged.exists() && original.exists());
    f.finish();
}

#[tokio::test]
async fn delete_local_products_keeps_other_batch_groups_without_recreating_deleted_group() {
    let f = Fixture::new();
    for name in ["A", "B"] {
        fs::create_dir(f.path.join(name)).unwrap();
        fs::write(f.path.join(name).join("authored.txt"), name.as_bytes()).unwrap();
    }
    let result = f.service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[f.path.join("A"),f.path.join("B")],"mode":"batch","rootId":"root"})).unwrap();
    let mut job = f
        .service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    f.service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    f.service.put_job(&mut job, "completed", None).unwrap();
    let removed = job["productIds"][0].as_str().unwrap().to_owned();
    let retained = job["productIds"][1].as_str().unwrap().to_owned();
    f.service
        .delete_local_products(&json!({"productIds":[removed]}), None)
        .await
        .unwrap();
    let mut pruned = f
        .service
        .catalogue
        .get("job", job["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(pruned["productIds"], json!([retained]));
    assert_eq!(pruned["completedGroupIndexes"], json!([0]));
    assert_eq!(pruned["localPlan"]["groups"].as_array().unwrap().len(), 1);
    f.service
        .run_local_import(&mut pruned, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    assert!(f
        .service
        .catalogue
        .get("product", &removed)
        .unwrap()
        .is_none());
    assert!(f
        .service
        .catalogue
        .get("product", &retained)
        .unwrap()
        .is_some());
    f.finish();
}

#[cfg(windows)]
#[tokio::test]
async fn delete_local_products_rejects_directory_junction_without_touching_target() {
    let f = Fixture::new();
    let (a, dir, _) = f.import("Junction");
    let outside = f.path.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("preserved.txt"), b"outside authored file").unwrap();
    let link = dir.join("junction");
    create_junction(&link, &outside);
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "local_delete_unsafe_path"
    );
    assert_eq!(
        fs::read(outside.join("preserved.txt")).unwrap(),
        b"outside authored file"
    );
    fs::remove_dir(link).unwrap();
    f.finish();
}

#[cfg(windows)]
#[tokio::test]
async fn delete_local_products_rejects_root_ancestor_junction_before_canonicalization() {
    let f = Fixture::new();
    let (a, dir, original) = f.import("LinkedRoot");
    let link = f.path.join("alias");
    create_junction(&link, &f.path.join("library"));
    fs::create_dir_all(f.path.join("library/nested")).unwrap();
    f.service
        .catalogue
        .put(
            "root",
            "root",
            json!({"id":"root","path":link.join("nested"),"status":"online"}),
        )
        .unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({"productIds":[a]}), None)
            .await
            .unwrap_err()
            .to_string(),
        "local_delete_unsafe_path"
    );
    assert!(dir.exists() && original.exists());
    fs::remove_dir(link).unwrap();
    f.finish();
}

#[cfg(windows)]
#[tokio::test]
async fn delete_local_products_cleanup_failure_survives_clear_and_retries_owned_staging() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    let f = Fixture::new();
    let (a, directory, original) = f.import("Cleanup");
    let id = uuid::Uuid::new_v4().to_string();
    let directories = f.service.delete_plan(&[a.clone()], &id).unwrap();
    let (_, staged) = f.service.delete_paths(&id, 0, &directories[0]).unwrap();
    let job = json!({"id":id,"type":JOB_TYPE,"originDeviceId":f.service.catalogue.device_id(),"productIds":[a],"state":"needsAction","localDelete":{"directories":directories,"recordsDeleted":true}});
    fs::rename(&directory, &staged).unwrap();
    f.service
        .catalogue
        .delete_local_products(&[a.clone()], job)
        .unwrap();
    let managed_file = fs::read_dir(&staged)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    // Deny deletion only after staging, so the real Windows cleanup fails
    // without preventing the preceding rename or using a production failpoint.
    let cleanup_blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&managed_file)
        .unwrap();
    assert_eq!(
        f.service
            .delete_local_products(&json!({}), Some(&id))
            .await
            .unwrap_err()
            .to_string(),
        "local_delete_cleanup_failed"
    );
    assert!(f.service.catalogue.get("product", &a).unwrap().is_none() && original.exists());
    assert!(managed_file.exists());
    let job = f
        .service
        .catalogue
        .list("job")
        .unwrap()
        .into_iter()
        .find(|j| j["type"] == JOB_TYPE)
        .unwrap();
    assert_eq!(job["localDelete"]["recordsDeleted"], true);
    f.service
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    assert!(f
        .service
        .catalogue
        .get("job", job["id"].as_str().unwrap())
        .unwrap()
        .is_some());
    drop(cleanup_blocker);
    f.service
        .delete_local_products(&json!({}), job["id"].as_str())
        .await
        .unwrap();
    assert!(!staged.exists() && !directory.exists());
    f.finish();
}
