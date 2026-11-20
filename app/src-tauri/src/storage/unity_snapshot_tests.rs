
use super::*;
use crate::{booth::BoothService, catalogue::Catalogue, stores::StoreService};
use std::sync::atomic::Ordering;

fn fixture() -> (StorageService, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("nna-unity-snapshot-synthetic-{}", Uuid::new_v4()));
    let catalogue = Catalogue::open(&directory.join("data")).unwrap();
    let booth = Arc::new(BoothService::new(catalogue.clone()).unwrap());
    let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
    let service = StorageService::new(catalogue, stores).unwrap();
    let root = directory.join("library");
    fs::create_dir_all(&root).unwrap();
    service
        .catalogue
        .put(
            "root",
            "synthetic-root",
            json!({"id":"synthetic-root","path":root,"status":"online"}),
        )
        .unwrap();
    service.catalogue.put("product", "local:synthetic", json!({"id":"local:synthetic","provider":"local","custom":true,"title":"Authored Bridge fixture","localManifest":{"complete":true,"fileIds":["package","archive"],"directories":[]}})).unwrap();
    (service, root)
}

fn seed_file(service: &StorageService, root: &Path, id: &str, name: &str, bytes: &[u8]) -> Value {
    let remote = json!({"id":id,"productId":"local:synthetic","provider":"local","originKind":"local","name":name,"revision":"r1"});
    service
        .catalogue
        .put("remote_file", id, remote.clone())
        .unwrap();
    let relative = format!("assets/Authored--synthetic/original/{id}");
    let path = root.join(&relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    let (hash, size) = io::hash_file(&path).unwrap();
    let artifact = json!({"id":artifact_id(&remote),"productId":"local:synthetic","fileId":id,"revision":"r1","rootId":"synthetic-root","productDirectory":"assets/Authored--synthetic","relativePath":relative,"hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true});
    service
        .catalogue
        .put(
            "artifact",
            artifact["id"].as_str().unwrap(),
            artifact.clone(),
        )
        .unwrap();
    artifact
}

#[tokio::test]
async fn unity_snapshot_binds_original_and_inventory_member_and_releases_gate() {
    let (service, root) = fixture();
    let original = seed_file(
        &service,
        &root,
        "package",
        "Packages/Example.unitypackage",
        b"authored intact package",
    );
    let mut archive = seed_file(
        &service,
        &root,
        "archive",
        "Example.zip",
        b"authored retained archive",
    );
    let base = "assets/Authored--synthetic/extracted/Example";
    fs::create_dir_all(root.join(base).join("Models")).unwrap();
    fs::create_dir_all(root.join(base).join("Empty")).unwrap();
    fs::write(
        root.join(base).join("Models/example.fbx"),
        b"authored model",
    )
    .unwrap();
    let (hash, size) = io::hash_file(&root.join(base).join("Models/example.fbx")).unwrap();
    archive["extractedPath"] = json!(base);
    archive["extractedInventory"] = json!([
        {"relativePath":"Models/example.fbx","hash":hash,"sizeBytes":size},
        {"relativePath":"Empty","hash":"","sizeBytes":0,"isDirectory":true}
    ]);
    service
        .catalogue
        .put("artifact", archive["id"].as_str().unwrap(), archive.clone())
        .unwrap();
    let products = vec!["local:synthetic".to_owned()];
    let leaves = service.list_unity_inventory(&products).await.unwrap();
    assert_eq!(leaves.len(), 3, "directories are not transferable leaves");
    let package = leaves
        .iter()
        .find(|leaf| leaf.file_id == "package")
        .unwrap();
    assert_eq!(package.kind, "unitypackage");
    assert_eq!(package.member_path, "");
    assert_eq!(package.relative_path, "Packages/Example.unitypackage");
    let member = leaves
        .iter()
        .find(|leaf| !leaf.member_path.is_empty())
        .unwrap();
    let retained = leaves
        .iter()
        .find(|leaf| leaf.file_id == "archive" && leaf.member_path.is_empty())
        .unwrap();
    assert_ne!(member.leaf_ref, retained.leaf_ref);
    assert_eq!(member.relative_path, "Models/example.fbx");
    // Only a package is read in place by Unity's own importer.
    assert_eq!(
        service
            .unity_package_path(&products, &package.leaf_ref)
            .await
            .unwrap(),
        Some(root.join(original["relativePath"].as_str().unwrap()))
    );
    assert_eq!(
        service
            .unity_package_path(&products, &member.leaf_ref)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        service
            .unity_package_path(&products, "missing")
            .await
            .unwrap_err()
            .to_string(),
        "unity_leaf_stale"
    );
    let request = Uuid::new_v4().to_string();
    let snapshot = service
        .prepare_unity_snapshot(
            &request,
            &products,
            &[package.leaf_ref.clone(), member.leaf_ref.clone()],
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(snapshot.entries.len(), 2);
    assert!(
        service.operations.try_write().is_ok(),
        "snapshot retains no operation guard"
    );
    for entry in &snapshot.entries {
        assert_eq!(entry.path.parent(), Some(snapshot.directory.as_path()));
        assert_eq!(
            entry.path.file_name().unwrap().to_str().unwrap(),
            entry.entry_id
        );
        io::verify(&entry.path, &entry.leaf.sha256, entry.leaf.bytes).unwrap();
    }
    assert_eq!(
        fs::read(&snapshot.entries[0].path).unwrap(),
        b"authored intact package"
    );
    // The immutable copy remains unchanged when its source changes later.
    fs::write(
        root.join(original["relativePath"].as_str().unwrap()),
        b"changed after snapshot",
    )
    .unwrap();
    assert_eq!(
        fs::read(&snapshot.entries[0].path).unwrap(),
        b"authored intact package"
    );
    // A root-owned receipt is outside cleanup's exact owned snapshot.
    let receipt = snapshot.directory.parent().unwrap().join("receipt.json");
    fs::write(&receipt, b"authored receipt").unwrap();
    service.cleanup_unity_snapshot(&request).unwrap();
    assert!(!snapshot.directory.exists());
    assert!(receipt.exists());
    assert!(root.join(base).join("Models/example.fbx").exists());
    service.cleanup_unity_snapshot(&request).unwrap();
    assert!(service.cleanup_unity_snapshot("../library").is_err());
    // A later corrupt member must also remove the already verified first
    // snapshot copy, while retaining both registered managed inputs.
    fs::write(
        root.join(original["relativePath"].as_str().unwrap()),
        b"authored intact package",
    )
    .unwrap();
    fs::write(
        root.join(base).join("Models/example.fbx"),
        b"damaged model!",
    )
    .unwrap();
    let failed_request = Uuid::new_v4().to_string();
    let failed = service
        .prepare_unity_snapshot(
            &failed_request,
            &products,
            &[package.leaf_ref.clone(), member.leaf_ref.clone()],
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert_eq!(failed.err().unwrap().to_string(), "file_integrity_failed");
    assert!(!service
        .unity_snapshot_directory(&failed_request)
        .unwrap()
        .exists());
    assert!(root
        .join(original["relativePath"].as_str().unwrap())
        .exists());
    assert_eq!(
        fs::read(root.join(base).join("Models/example.fbx")).unwrap(),
        b"damaged model!"
    );
}

#[tokio::test]
async fn unity_inventory_offers_a_file_saved_under_its_windows_name() {
    let (service, root) = fixture();
    seed_file(
        &service,
        &root,
        "package",
        "Outfit: Summer?.unitypackage",
        b"authored mapped package",
    );
    let leaves = service
        .list_unity_inventory(&["local:synthetic".to_owned()])
        .await
        .unwrap();
    let leaf = leaves
        .iter()
        .find(|leaf| leaf.file_id == "package")
        .unwrap();
    assert_eq!(leaf.relative_path, "Outfit： Summer？.unitypackage");
    assert_eq!(leaf.kind, "unitypackage");
}

#[tokio::test]
async fn unity_inventory_offers_store_files_kept_outside_the_current_list() {
    let (service, root) = fixture();
    let c = &service.catalogue;
    c.put(
        "product",
        "900001",
        json!({"id":"900001","store":"BOOTH","title":"Authored store product"}),
    )
    .unwrap();
    c.put("entitlement", "grant", json!({"id":"grant","productId":"900001","accountId":"authored-account","fileIds":["current"],"manifestComplete":true})).unwrap();
    // The store dropped `dropped` after its download; the user added the third.
    // `alias` is an older descriptor of the downloadable `current` still offers.
    for (id, source, origin) in [
        ("current", "700001", None),
        ("dropped", "700000", None),
        ("alias", "700001", None),
        ("local-file:added", "", Some("local")),
    ] {
        let mut remote = json!({"id":id,"productId":"900001","accountId":"authored-account","sourceFileId":source,"name":format!("{}.unitypackage", id.replace(':', "-")),"revision":"r1"});
        if let Some(origin) = origin {
            remote["originKind"] = json!(origin);
        }
        c.put("remote_file", id, remote.clone()).unwrap();
        let relative = format!(
            "assets/Authored--900001/{}.unitypackage",
            id.replace(':', "-")
        );
        let path = root.join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("authored {id}")).unwrap();
        let (hash, size) = io::hash_file(&path).unwrap();
        let artifact = json!({"id":artifact_id(&remote),"productId":"900001","fileId":id,"revision":"r1","rootId":"synthetic-root","productDirectory":"assets/Authored--900001","relativePath":relative,"hash":hash,"sizeBytes":size,"status":"ready","originalAvailable":true});
        c.put(
            "artifact",
            artifact["id"].as_str().unwrap(),
            artifact.clone(),
        )
        .unwrap();
    }
    // A dropped file that was never downloaded has nothing to import.
    c.put(
        "remote_file",
        "gone",
        json!({"id":"gone","productId":"900001","name":"gone.unitypackage","revision":"r1"}),
    )
    .unwrap();
    let leaves = service
        .list_unity_inventory(&["900001".to_owned()])
        .await
        .unwrap();
    let mut ids: Vec<_> = leaves.iter().map(|leaf| leaf.file_id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["current", "dropped", "local-file:added"]);
}

#[tokio::test]
async fn unity_snapshot_rejects_stale_corrupt_collision_and_cancel_preserving_sources() {
    let (service, root) = fixture();
    let mut original = seed_file(
        &service,
        &root,
        "package",
        "Example.unitypackage",
        b"authored payload",
    );
    let products = vec!["local:synthetic".to_owned()];
    let old = service
        .list_unity_inventory(&products)
        .await
        .unwrap()
        .remove(0);
    original["hash"] = json!(io::key("changed binding"));
    service
        .catalogue
        .put(
            "artifact",
            original["id"].as_str().unwrap(),
            original.clone(),
        )
        .unwrap();
    let stale_request = Uuid::new_v4().to_string();
    let stale = service
        .prepare_unity_snapshot(
            &stale_request,
            &products,
            &[old.leaf_ref],
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert_eq!(stale.err().unwrap().to_string(), "unity_leaf_stale");
    assert!(!service
        .unity_snapshot_directory(&stale_request)
        .unwrap()
        .exists());
    let original = seed_file(
        &service,
        &root,
        "package",
        "Example.unitypackage",
        b"authored payload",
    );
    let good = service
        .list_unity_inventory(&products)
        .await
        .unwrap()
        .remove(0);
    let source = root.join(original["relativePath"].as_str().unwrap());
    fs::write(&source, b"damaged! payload").unwrap(); // same length; listing alone cannot prove integrity
    let corrupt_request = Uuid::new_v4().to_string();
    let corrupt = service
        .prepare_unity_snapshot(
            &corrupt_request,
            &products,
            &[good.leaf_ref.clone()],
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert_eq!(corrupt.err().unwrap().to_string(), "file_integrity_failed");
    assert!(!service
        .unity_snapshot_directory(&corrupt_request)
        .unwrap()
        .exists());
    assert_eq!(fs::read(&source).unwrap(), b"damaged! payload");
    seed_file(
        &service,
        &root,
        "package",
        "Example.unitypackage",
        b"authored payload",
    );
    seed_file(
        &service,
        &root,
        "archive",
        "example.UNITYPACKAGE",
        b"authored second payload",
    );
    let leaves = service.list_unity_inventory(&products).await.unwrap();
    let collision = service
        .prepare_unity_snapshot(
            &Uuid::new_v4().to_string(),
            &products,
            &leaves
                .iter()
                .map(|leaf| leaf.leaf_ref.clone())
                .collect::<Vec<_>>(),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert_eq!(collision.err().unwrap().to_string(), "unity_path_collision");
    let cancel = Arc::new(AtomicBool::new(false));
    // Pause at the existing storage gate and cancel before copying starts.
    let gate = service.operations.write().await;
    let worker = service.clone();
    let request = Uuid::new_v4().to_string();
    let requested = request.clone();
    let cancelling = cancel.clone();
    let task = tokio::spawn(async move {
        worker
            .prepare_unity_snapshot(&requested, &products, &[good.leaf_ref], cancelling)
            .await
    });
    tokio::task::yield_now().await;
    cancel.store(true, Ordering::Release);
    drop(gate);
    assert_eq!(task.await.unwrap().err().unwrap().to_string(), "cancelled");
    assert!(!service.unity_snapshot_directory(&request).unwrap().exists());
    assert!(source.exists());
    assert!(service.operations.try_write().is_ok());
}

#[test]
fn unity_snapshot_collision_detects_file_parent_even_with_intervening_name() {
    let entry = |relative: &str| ResolvedLeaf {
        leaf: UnityLeaf {
            leaf_ref: String::new(),
            product_id: String::new(),
            product_title: String::new(),
            file_id: String::new(),
            source_name: String::new(),
            variant_name: String::new(),
            revision: String::new(),
            member_path: String::new(),
            display_name: String::new(),
            relative_path: relative.into(),
            kind: "file".into(),
            bytes: 0,
            sha256: String::new(),
        },
        source: PathBuf::new(),
        remote: Value::Null,
        artifact: Arc::new(Value::Null),
        root: Value::Null,
    };
    assert!(require_distinct_paths(&[
        entry("Model"),
        entry("model-a.txt"),
        entry("model/mesh.fbx")
    ])
    .is_err());
    assert!(require_distinct_paths(&[entry("A/mesh.fbx"), entry("B/mesh.fbx")]).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn unity_snapshot_never_reads_linked_managed_leaf() {
    let (service, root) = fixture();
    let original = seed_file(
        &service,
        &root,
        "package",
        "Example.unitypackage",
        b"authored payload",
    );
    let products = vec!["local:synthetic".into()];
    let leaf = service
        .list_unity_inventory(&products)
        .await
        .unwrap()
        .remove(0);
    let source = root.join(original["relativePath"].as_str().unwrap());
    let external = root.parent().unwrap().join("external-authored.bin");
    fs::write(&external, b"authored payload").unwrap();
    fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(&external, &source).unwrap();
    assert!(service
        .list_unity_inventory(&products)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        service
            .prepare_unity_snapshot(
                &Uuid::new_v4().to_string(),
                &products,
                &[leaf.leaf_ref],
                Arc::new(AtomicBool::new(false))
            )
            .await
            .err()
            .unwrap()
            .to_string(),
        "unity_leaf_stale"
    );
    assert_eq!(fs::read(external).unwrap(), b"authored payload");
}
