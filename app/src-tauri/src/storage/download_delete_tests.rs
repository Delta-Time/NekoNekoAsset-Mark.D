use super::*;

struct Fixture {
    path: PathBuf,
    library: PathBuf,
    service: StorageService,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nna-download-delete-authored-{}",
            uuid::Uuid::new_v4()
        ));
        let library = path.join("library");
        fs::create_dir_all(&library).unwrap();
        let catalogue = Catalogue::open(&path.join("data")).unwrap();
        let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
        let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
        let service = StorageService::new(catalogue, stores).unwrap();
        service
            .catalogue
            .put(
                "root",
                "root",
                json!({"id":"root","path":library,"status":"online"}),
            )
            .unwrap();
        service
            .catalogue
            .put(
                "account",
                "account",
                json!({"id":"account","store":"BOOTH","status":"connected"}),
            )
            .unwrap();
        Self {
            path,
            library,
            service,
        }
    }
    /// One authored store product whose files are all downloaded.
    fn product(&self, id: &str, files: &[&str]) -> String {
        let c = &self.service.catalogue;
        c.put("product",id,json!({"id":id,"provider":"booth","title":format!("Authored {id}"),"category":"Clothes"})).unwrap();
        let directory = format!("assets/{}", io::readable_component(id, id));
        for name in files {
            let file = format!("{id}-{name}");
            c.put(
                "remote_file",
                &file,
                json!({"id":file,"productId":id,"accountId":"account","name":name,"revision":"r1"}),
            )
            .unwrap();
            let relative = format!("{directory}/{name}--dir/{name}");
            let path = self.library.join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("authored bytes {file}")).unwrap();
            let (hash, size) = io::hash_file(&path).unwrap();
            let artifact = format!("artifact-{file}");
            c.put("artifact",&artifact,json!({"id":artifact,"productId":id,"fileId":file,"rootId":"root","relativePath":relative,"productDirectory":directory,"revision":"r1","hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true})).unwrap();
        }
        let ids: Vec<_> = files.iter().map(|name| format!("{id}-{name}")).collect();
        c.put("entitlement",&format!("entitlement-{id}"),json!({"id":format!("entitlement-{id}"),"productId":id,"accountId":"account","fileIds":ids,"manifestComplete":true})).unwrap();
        directory
    }
    fn file_status(&self, product: &str, file: &str) -> String {
        let details = self
            .service
            .catalogue
            .command("product_details", json!({"id":product}))
            .unwrap();
        details["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == file)
            .unwrap()["status"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    /// Queues a removal and runs every waiting one as the storage worker would; answers the
    /// files whose bytes are gone.
    async fn delete(&self, args: Value) -> Result<Vec<String>> {
        self.service.delete_downloads(&args).await?;
        let mut deleted = Vec::new();
        while let Some(job) = self.service.queued_delete()? {
            deleted.extend(
                self.service
                    .run_delete_job(job["id"].as_str().unwrap(), || {})?,
            );
        }
        Ok(deleted)
    }
    fn finish(self) {
        let Self { path, service, .. } = self;
        drop(service);
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nna-download-delete-authored-"));
        remove_checked_tree(&path).unwrap();
    }
}

#[tokio::test]
async fn product_delete_keeps_files_the_store_cannot_provide_again() {
    let f = Fixture::new();
    let directory = f.product("alpha", &["New.zip", "Old.zip", "Gone.zip"]);
    let c = &f.service.catalogue;
    // The store dropped Old.zip after its download and Gone.zip after its
    // listing changed, which hides Gone.zip's row; the user added Mine.zip.
    c.put("entitlement","entitlement-alpha",json!({"id":"entitlement-alpha","productId":"alpha","accountId":"account","fileIds":["alpha-New.zip"],"manifestComplete":true})).unwrap();
    let mut gone = c.get("remote_file", "alpha-Gone.zip").unwrap().unwrap();
    gone["revision"] = json!("r2");
    c.put("remote_file", "alpha-Gone.zip", gone).unwrap();
    let hidden = f.library.join(&directory).join("Gone.zip--dir/Gone.zip");
    let relative = format!("{directory}/Mine.zip");
    let mine = f.library.join(&relative);
    fs::write(&mine, b"authored user copy").unwrap();
    let (hash, size) = io::hash_file(&mine).unwrap();
    let revision = format!("sha256:{hash}");
    c.put("remote_file","local-file:mine",json!({"id":"local-file:mine","productId":"alpha","provider":"local","originKind":"local","name":"Mine.zip","revision":revision,"sizeBytes":size})).unwrap();
    c.put("artifact","artifact-mine",json!({"id":"artifact-mine","productId":"alpha","fileId":"local-file:mine","rootId":"root","relativePath":relative,"productDirectory":directory,"revision":revision,"hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true})).unwrap();
    let old = f.library.join(&directory).join("Old.zip--dir/Old.zip");

    let deleted = f.delete(json!({"productIds":["alpha"]})).await.unwrap();
    assert_eq!(deleted, ["alpha-New.zip"]);
    assert!(old.exists() && mine.exists() && hidden.exists());
    assert_eq!(f.file_status("alpha", "alpha-Old.zip"), "ready");
    assert_eq!(f.file_status("alpha", "local-file:mine"), "ready");

    // Each file's own delete removes them; an added file leaves with its record.
    f.delete(json!({"fileId":"local-file:mine"})).await.unwrap();
    f.delete(json!({"fileId":"alpha-Old.zip"})).await.unwrap();
    assert!(!old.exists() && !mine.exists());
    assert!(c.get("remote_file", "local-file:mine").unwrap().is_none());
    assert!(c.get("remote_file", "alpha-Old.zip").unwrap().is_some());
    let details = c.command("product_details", json!({"id":"alpha"})).unwrap();
    let ids: Vec<_> = details["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["alpha-New.zip"]);
    f.finish();
}

#[tokio::test]
async fn product_delete_keeps_a_downloaded_file_its_store_refused() {
    let f = Fixture::new();
    let directory = f.product("beta", &["New.zip", "Old.zip"]);
    let c = &f.service.catalogue;
    // The grant still names Old.zip, but its last download was refused as no longer
    // offered; the refusal record names the fresh path that attempt had planned.
    let pending = format!("pending-{}", io::key("beta-Old.zip"));
    c.put("artifact",&pending,json!({"id":pending,"productId":"beta","fileId":"beta-Old.zip","rootId":"root","relativePath":format!("{directory}/Old.zip--dir-1/Old.zip"),"productDirectory":directory,"revision":null,"status":"notDownloaded","errorCode":"store_file_unavailable","originalAvailable":false})).unwrap();
    let old = f.library.join(&directory).join("Old.zip--dir/Old.zip");
    let deleted = f.delete(json!({"productIds":["beta"]})).await.unwrap();
    assert_eq!(deleted, ["beta-New.zip"]);
    assert!(old.exists());
    let details = c.command("product_details", json!({"id":"beta"})).unwrap();
    let row = details["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "beta-Old.zip")
        .unwrap()
        .clone();
    assert_eq!(
        (row["kept"].clone(), row["status"].clone()),
        (json!(true), json!("ready"))
    );
    // Its own delete removes the bytes and keeps the refusal, so it leaves the list.
    f.delete(json!({"fileId":"beta-Old.zip"})).await.unwrap();
    assert!(!old.exists());
    assert!(c.get("artifact", &pending).unwrap().is_some());
    let details = c.command("product_details", json!({"id":"beta"})).unwrap();
    let ids: Vec<_> = details["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["beta-New.zip"]);
    f.finish();
}

#[tokio::test]
async fn product_delete_removes_downloads_and_keeps_records_and_user_files() {
    let f = Fixture::new();
    let directory = f.product("alpha", &["Body.unitypackage", "Texture.zip"]);
    f.product("beta", &["Other.unitypackage"]);
    // An extraction and a user-added file beside the managed outputs.
    let extracted = f.library.join(&directory).join("extracted/Texture--x");
    fs::create_dir_all(extracted.join("nested")).unwrap();
    fs::write(extracted.join("nested/texture.png"), b"authored texture").unwrap();
    let mut archive = f
        .service
        .catalogue
        .get("artifact", "artifact-alpha-Texture.zip")
        .unwrap()
        .unwrap();
    archive["extractedPath"] = json!(format!("{directory}/extracted/Texture--x"));
    f.service
        .catalogue
        .put("artifact", "artifact-alpha-Texture.zip", archive)
        .unwrap();
    fs::write(f.library.join(&directory).join("notes.txt"), b"user notes").unwrap();
    assert_eq!(f.file_status("alpha", "alpha-Body.unitypackage"), "ready");

    let deleted = f.delete(json!({"productIds":["alpha"]})).await.unwrap();
    assert_eq!(deleted, ["alpha-Body.unitypackage", "alpha-Texture.zip"]);
    assert!(!extracted.exists());
    assert!(!f
        .library
        .join(&directory)
        .join("Body.unitypackage--dir")
        .exists());
    assert_eq!(
        fs::read(f.library.join(&directory).join("notes.txt")).unwrap(),
        b"user notes"
    );
    assert!(f
        .service
        .catalogue
        .related("artifact", "alpha")
        .unwrap()
        .is_empty());
    assert_eq!(
        f.service
            .catalogue
            .related("remote_file", "alpha")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        f.file_status("alpha", "alpha-Body.unitypackage"),
        "notDownloaded"
    );
    assert_eq!(
        f.service
            .catalogue
            .get("download_intent", "alpha")
            .unwrap()
            .unwrap()["wantDownload"],
        false
    );
    // The unrelated product stays; download-all queues it but not the removed one.
    assert_eq!(
        f.service
            .catalogue
            .related("artifact", "beta")
            .unwrap()
            .len(),
        1
    );
    f.service
        .catalogue
        .command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    f.service
        .evaluate_automatic(&HashSet::from(["alpha".to_owned(), "beta".to_owned()]))
        .await
        .unwrap();
    let jobs: Vec<_> = f
        .service
        .catalogue
        .list("job")
        .unwrap()
        .into_iter()
        .filter(|job| job["type"] == "download")
        .collect();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["productId"], "beta");
    f.finish();
}

#[tokio::test]
async fn file_delete_keeps_bytes_another_file_still_records() {
    let f = Fixture::new();
    let directory = f.product("alpha", &["Body.unitypackage", "Copy.unitypackage"]);
    // Verified-content sharing: the second file records the first file's bytes.
    let shared = f
        .service
        .catalogue
        .get("artifact", "artifact-alpha-Body.unitypackage")
        .unwrap()
        .unwrap()["relativePath"]
        .clone();
    let mut copy = f
        .service
        .catalogue
        .get("artifact", "artifact-alpha-Copy.unitypackage")
        .unwrap()
        .unwrap();
    let own = f.library.join(copy["relativePath"].as_str().unwrap());
    copy["relativePath"] = shared.clone();
    f.service
        .catalogue
        .put("artifact", "artifact-alpha-Copy.unitypackage", copy)
        .unwrap();
    f.delete(json!({"fileId":"alpha-Body.unitypackage"}))
        .await
        .unwrap();
    assert!(f.library.join(shared.as_str().unwrap()).is_file());
    assert!(own.is_file(), "an unrecorded file is never removed");
    assert_eq!(
        f.file_status("alpha", "alpha-Body.unitypackage"),
        "notDownloaded"
    );
    assert_eq!(f.file_status("alpha", "alpha-Copy.unitypackage"), "ready");
    assert!(f
        .service
        .catalogue
        .get("download_intent", "alpha")
        .unwrap()
        .is_none());
    // Removing the last record also removes the shared bytes and empty folders.
    f.delete(json!({"fileId":"alpha-Copy.unitypackage"}))
        .await
        .unwrap();
    assert!(!f.library.join(shared.as_str().unwrap()).exists());
    assert!(f.library.join(&directory).exists() && own.is_file());
    f.finish();
}

#[tokio::test]
async fn delete_refuses_active_work_and_invalid_targets_without_removing_bytes() {
    let f = Fixture::new();
    let directory = f.product("alpha", &["Body.unitypackage"]);
    let bytes = f
        .library
        .join(&directory)
        .join("Body.unitypackage--dir/Body.unitypackage");
    for (id, job) in [
        (
            "download",
            json!({"id":"download","type":"download","productId":"alpha","state":"running"}),
        ),
        (
            "unity",
            json!({"id":"unity","type":"unity:import","productIds":["alpha"],"state":"queued"}),
        ),
    ] {
        f.service.catalogue.put("job", id, job).unwrap();
        assert_eq!(
            f.service
                .delete_downloads(&json!({"productIds":["alpha"]}))
                .await
                .unwrap_err()
                .to_string(),
            "storage_busy"
        );
        f.service.catalogue.delete("job", id).unwrap();
    }
    f.service
        .catalogue
        .put(
            "product",
            "local",
            json!({"id":"local","provider":"local","title":"Authored local"}),
        )
        .unwrap();
    for (args, code) in [
        (json!({"productIds":["local"]}), "invalid_arguments"),
        (json!({"productIds":["alpha","alpha"]}), "invalid_arguments"),
        (json!({"productIds":[]}), "invalid_arguments"),
        (json!({"productIds":["missing"]}), "product_missing"),
        (json!({"fileId":"missing"}), "file_missing"),
    ] {
        assert_eq!(
            f.service
                .delete_downloads(&args)
                .await
                .unwrap_err()
                .to_string(),
            code
        );
    }
    assert!(bytes.is_file());
    assert_eq!(f.file_status("alpha", "alpha-Body.unitypackage"), "ready");
    f.finish();
}

#[tokio::test]
async fn removals_wait_their_turn_and_hold_their_products_meanwhile() {
    let f = Fixture::new();
    f.product("alpha", &["Body.unitypackage"]);
    let directory = f.product("beta", &["Hair.unitypackage"]);
    let c = &f.service.catalogue;
    let job = |answer: Value| answer["jobIds"][0].as_str().unwrap().to_owned();
    let first = job(f
        .service
        .delete_downloads(&json!({"productIds":["beta"]}))
        .await
        .unwrap());
    std::thread::sleep(Duration::from_millis(2));
    let second = job(f
        .service
        .delete_downloads(&json!({"productIds":["alpha"]}))
        .await
        .unwrap());
    // Asking again answers the waiting removal, which also covers each file.
    assert_eq!(
        f.service
            .delete_downloads(&json!({"fileId":"alpha-Body.unitypackage"}))
            .await
            .unwrap()["jobIds"],
        json!([second])
    );
    // Nothing goes before the worker runs them. Their products show in progress, and no
    // download of them is queued meanwhile: one asked for is refused, not merged, and
    // download-all leaves them alone.
    assert_eq!(f.file_status("alpha", "alpha-Body.unitypackage"), "ready");
    assert_eq!(
        c.command("product_details", json!({"id":"alpha"})).unwrap()["status"],
        "processing"
    );
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":["alpha"]}))
            .await
            .unwrap_err()
            .to_string(),
        "storage_busy"
    );
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":["alpha"],"automatic":true}))
            .await
            .unwrap()["jobIds"],
        json!([])
    );
    // The oldest goes first, unless a Unity transfer holds its product's files.
    c.put(
        "job",
        "unity",
        json!({"id":"unity","type":"unity:import","productIds":["beta"],"state":"running"}),
    )
    .unwrap();
    assert_eq!(f.service.queued_delete().unwrap().unwrap()["id"], second);
    c.delete("job", "unity").unwrap();
    assert_eq!(f.service.queued_delete().unwrap().unwrap()["id"], first);
    // A cancelled one removes nothing; Retry queues it again as it was asked for.
    f.service.cancel_job(&first).unwrap();
    assert!(f
        .service
        .run_delete_job(&first, || unreachable!())
        .unwrap()
        .is_empty());
    assert!(f.library.join(&directory).exists());
    f.service.retry_delete(&first).await.unwrap();
    assert_eq!(c.get("job", &first).unwrap().unwrap()["state"], "queued");
    assert_eq!(
        f.service.run_delete_job(&first, || {}).unwrap(),
        ["beta-Hair.unitypackage"]
    );
    assert_eq!(c.get("job", &first).unwrap().unwrap()["state"], "completed");
    assert_eq!(
        f.file_status("beta", "beta-Hair.unitypackage"),
        "notDownloaded"
    );
    // Once it has run, its product takes downloads again.
    f.service.run_delete_job(&second, || {}).unwrap();
    assert_eq!(
        f.service
            .enqueue(&json!({"productIds":["alpha"]}))
            .await
            .unwrap()["jobIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.finish();
}

#[cfg(windows)]
#[tokio::test]
async fn a_file_held_open_is_kept_and_shown_missing_until_retried() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let directory = f.product("alpha", &["Body.unitypackage", "Hair.unitypackage"]);
    let held = f
        .library
        .join(&directory)
        .join("Body.unitypackage--dir/Body.unitypackage");
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&held)
        .unwrap();
    assert_eq!(
        f.delete(json!({"productIds":["alpha"]}))
            .await
            .unwrap_err()
            .to_string(),
        "download_delete_incomplete"
    );
    let jobs = f.service.catalogue.list("job").unwrap();
    assert_eq!(
        (&jobs[0]["state"], &jobs[0]["errorCode"]),
        (&json!("needsAction"), &json!("download_delete_incomplete"))
    );
    assert!(held.is_file());
    assert_eq!(f.file_status("alpha", "alpha-Body.unitypackage"), "missing");
    assert_eq!(
        f.file_status("alpha", "alpha-Hair.unitypackage"),
        "notDownloaded"
    );
    drop(handle);
    f.delete(json!({"fileId":"alpha-Body.unitypackage"}))
        .await
        .unwrap();
    assert!(!held.exists());
    assert_eq!(
        f.file_status("alpha", "alpha-Body.unitypackage"),
        "notDownloaded"
    );
    f.finish();
}
