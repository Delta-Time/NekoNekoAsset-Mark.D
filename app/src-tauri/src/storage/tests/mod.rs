use super::*;
use crate::test_dir::TestDir;

mod downloads;
mod extraction;
mod library;
mod local_import;

struct Fixture {
    service: StorageService,
    // Last, so the service closes SQLite before the folder is removed.
    path: TestDir,
}

impl Fixture {
    fn new() -> Self {
        let path = TestDir::new("storage-synthetic");
        let catalogue = Catalogue::open(&path.join("data")).unwrap();
        let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
        let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
        let service = StorageService::new(catalogue, stores).unwrap();
        std::fs::create_dir_all(path.join("library")).unwrap();
        service
            .catalogue
            .put(
                "root",
                "test-root",
                json!({"id":"test-root","path":path.join("library"),"status":"online"}),
            )
            .unwrap();
        Self { path, service }
    }
    fn seed(&self, count: usize, complete: bool) {
        let c = &self.service.catalogue;
        c.put(
            "product",
            "42",
            json!({"id":"42","title":"Synthetic clothing","category":"Clothes"}),
        )
        .unwrap();
        c.put(
            "account",
            "test-account",
            json!({"id":"test-account","store":"BOOTH","status":"connected"}),
        )
        .unwrap();
        let mut ids = Vec::new();
        for n in 0..count {
            let id = format!("test-file-{n}");
            c.put("remote_file",&id,json!({"id":id,"productId":"42","accountId":"test-account","sourceFileId":n.to_string(),"name":format!("fixture-{n}.unitypackage"),"revision":"r1"})).unwrap();
            ids.push(id);
        }
        c.put("entitlement","test-entitlement",json!({"id":"test-entitlement","productId":"42","accountId":"test-account","fileIds":ids,"manifestComplete":complete})).unwrap();
    }
    fn artifact(&self, index: usize) -> Value {
        let file = self
            .service
            .catalogue
            .get("remote_file", &format!("test-file-{index}"))
            .unwrap()
            .unwrap();
        let id = artifact_id(&file);
        let directory = format!(
            "assets/{}",
            io::readable_component("Synthetic clothing", "42")
        );
        let relative = format!("{directory}/fixture-{index}/fixture-{index}.unitypackage");
        let path = self.path.join("library").join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"synthetic unity package bytes").unwrap();
        let (hash, size) = io::hash_file(&path).unwrap();
        let artifact = json!({"id":id,"fileId":file["id"],"productId":"42","rootId":"test-root","relativePath":relative,"productDirectory":directory,"revision":"r1","hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true});
        self.service
            .catalogue
            .put("artifact", &id, artifact.clone())
            .unwrap();
        artifact
    }
    fn extracted_archive(&self, index: usize, retained: bool) -> (Value, PathBuf, PathBuf) {
        let mut file = self
            .service
            .catalogue
            .get("remote_file", &format!("test-file-{index}"))
            .unwrap()
            .unwrap();
        file["name"] = json!(format!("ProductA-{index}.zip"));
        self.service
            .catalogue
            .put("remote_file", file["id"].as_str().unwrap(), file.clone())
            .unwrap();
        let product_relative = format!(
            "assets/{}",
            io::readable_component("Synthetic clothing", "42")
        );
        let original_relative =
            format!("{product_relative}/revision-{index}/original/ProductA-{index}.zip");
        let extracted_relative = format!("{product_relative}/extracted/ProductA-{index}");
        let original = self.path.join("library").join(&original_relative);
        let extracted = self.path.join("library").join(&extracted_relative);
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::create_dir_all(extracted.join("empty")).unwrap();
        std::fs::write(extracted.join("model.fbx"), b"verified synthetic model").unwrap();
        if retained {
            std::fs::write(&original, b"synthetic complete archive").unwrap();
        }
        let (hash, size) = io::hash_file(&extracted.join("model.fbx")).unwrap();
        let id = artifact_id(&file);
        let artifact = json!({"id":id,"fileId":file["id"],"productId":"42","rootId":"test-root","relativePath":original_relative,"productDirectory":product_relative,"revision":"r1","hash":io::key("synthetic complete archive"),"sizeBytes":b"synthetic complete archive".len(),"status":"ready","originalAvailable":retained,"extractedPath":extracted_relative,"extractedInventory":[{"relativePath":"model.fbx","hash":hash,"sizeBytes":size},{"relativePath":"empty","hash":"","sizeBytes":0,"isDirectory":true}]});
        self.service
            .catalogue
            .put("artifact", &id, artifact.clone())
            .unwrap();
        (artifact, original, extracted)
    }
    /// Authored bytes begin at the verified-transfer boundary: no HTTP, login or
    /// resolver is involved. Use production binding/guarded publication and
    /// partial cleanup before exercising the actual native extraction lifecycle.
    fn publish_authored_download(
        &self,
        index: usize,
        name: &str,
        bytes: &[u8],
    ) -> (Value, PathBuf) {
        use std::io::Write;
        let file_id = format!("test-file-{index}");
        let mut observed = self
            .service
            .catalogue
            .get("remote_file", &file_id)
            .unwrap()
            .unwrap();
        observed["name"] = json!(name);
        observed["revision"] = json!("unknown");
        observed["requiresContentVerification"] = json!(true);
        observed["sourceObservationToken"] = json!(format!("authored-source-{index}"));
        self.service
            .catalogue
            .put("remote_file", &file_id, observed.clone())
            .unwrap();
        let root = self.path.join("library");
        let part = io::safe_join(
            &root,
            &format!(".nna-partials/pending-{}.part", io::key(&file_id)),
        )
        .unwrap();
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        let _partial = io::PartialGuard::new(part.clone());
        let mut output = std::fs::File::create(&part).unwrap();
        output.write_all(bytes).unwrap();
        output.sync_all().unwrap();
        drop(output);
        let (hash, size) = io::hash_file(&part).unwrap();
        let current = transfer::bind_downloaded_file(
            observed.clone(),
            format!("sha256:{hash}"),
            "contentHash".into(),
            size,
        );
        let directory = self.service.managed_product_directory("42").unwrap();
        let source_directory =
            io::unused_relative(&root, &format!("{directory}/download-{index}")).unwrap();
        let relative = format!("{source_directory}/original/{name}");
        let destination = io::safe_join(&root, &relative).unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::rename(&part, &destination).unwrap();
        let id = artifact_id(&current);
        let artifact = json!({"id":id,"productId":"42","fileId":file_id,"rootId":"test-root","relativePath":relative,"productDirectory":directory,"revision":current["revision"],"hash":hash,"sizeBytes":size,"status":"downloaded","originalAvailable":true});
        let pending_id = format!("pending-{}", io::key(&file_id));
        self.service
            .catalogue
            .put(
                "artifact",
                &pending_id,
                json!({"id":pending_id,"productId":"42","fileId":file_id,"rootId":"test-root","relativePath":relative,"status":"downloading","originalAvailable":false}),
            )
            .unwrap();
        let prior = self.service.catalogue.get("artifact", &id).unwrap();
        self.service
            .commit_download_records(
                vec![
                    ("remote_file".into(), file_id.clone(), current),
                    ("artifact".into(), id.clone(), artifact.clone()),
                ],
                vec![
                    ("remote_file".into(), file_id.clone(), Some(observed)),
                    ("artifact".into(), id, prior),
                ],
            )
            .unwrap();
        self.service
            .delete_pending_download_record(&file_id)
            .unwrap();
        assert!(self
            .service
            .catalogue
            .get("artifact", &pending_id)
            .unwrap()
            .is_none());
        (artifact, destination)
    }
}

fn authored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        let options = zip::write::SimpleFileOptions::default();
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn local_job(f: &Fixture, sources: &[PathBuf], mode: &str) -> Value {
    let result = f.service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4().to_string(),"mode":mode,"sources":sources,"category":"Clothes","name":"Synthetic local product"})).unwrap();
    f.service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn storage_requests_give_back_the_arguments_they_were_sent() {
    // Downloads keep selectedFileIds and a local import fingerprints its arguments, so
    // null stays apart from missing; parsing checks the round trip in test builds.
    let draft = uuid::Uuid::new_v4().to_string();
    for (action, args) in [
        (
            "import_local",
            json!({"draftId":draft,"mode":"single","sources":["C:/Authored/a.zip"],"name":"Authored","category":"Clothes","categoryManual":true,"tags":["VRChat"],"thumbnailUrl":null}),
        ),
        (
            "import_local",
            json!({"draftId":draft,"mode":"batch","sources":["C:/Authored/a.zip"],"rootId":"r","name":"","category":"Hair","categoryManual":false,"tags":[],"sourceUrl":"https://booth.pm/ja/items/1","thumbnailPath":"a.png","thumbnailUrl":"https://booth.pximg.net/a.png","products":[{"source":"C:/Authored/a.zip","name":"A","category":"Hair","categoryManual":false,"tags":[],"sourceUrl":"","thumbnailUrl":null}]}),
        ),
        (
            "local_files",
            json!({"productId":"local","draftId":draft,"addSources":["C:/Authored/b.png"],"removeFileIds":[]}),
        ),
        ("delete_local_products", json!({"productIds":["local"]})),
        ("enqueue_products", json!({"productIds":["1"]})),
        (
            "enqueue_products",
            json!({"productIds":["1"],"rootId":"r","selectedFileIds":null,"redownload":true}),
        ),
        ("enqueue_file", json!({"fileId":"f","redownload":false})),
        ("cancel_job", json!({"jobId":"j"})),
        ("cancel_downloads", json!({})),
        ("delete_downloads", json!({"productIds":["1"],"fileId":"f"})),
        ("retry_job", json!({"jobId":"j"})),
        ("retry_job", json!({"fileId":"f"})),
        ("discard_import", json!({"jobId":"j"})),
        ("configure_root", json!({"path":"D:/Authored"})),
        ("migrate_root", json!({"rootId":"r","path":"D:/Moved"})),
        ("cleanup_old_root", json!({"rootId":"r"})),
        ("export_zip", json!({"productIds":["1"],"path":"D:/a.zip"})),
        ("product_availability", json!({"productId":"1","extra":1.5})),
        ("root_usage", json!({})),
        ("open_file", json!({"fileId":"f"})),
        ("reset_app", json!({"deleteLibrary":false})),
    ] {
        assert_eq!(
            api::StorageRequest::parse(action, args.clone())
                .unwrap()
                .args(),
            args
        );
    }
}
