use super::*;

fn fixture() -> (PathBuf, StorageService) {
    let path =
        std::env::temp_dir().join(format!("nna-local-015-authored-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path.join("data")).unwrap();
    let booth = Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap());
    let stores = Arc::new(StoreService::new(catalogue.clone(), booth).unwrap());
    let service = StorageService::new(catalogue, stores).unwrap();
    fs::create_dir_all(path.join("library")).unwrap();
    service
        .catalogue
        .put(
            "root",
            "authored-root",
            json!({"id":"authored-root","path":path.join("library"),"status":"online"}),
        )
        .unwrap();
    (path, service)
}

#[test]
fn local_loose_names_and_batch_metadata_are_independent() {
    let (path, service) = fixture();
    fs::create_dir_all(path.join("A")).unwrap();
    fs::create_dir_all(path.join("B")).unwrap();
    fs::write(path.join("A/readme.txt"), b"authored A").unwrap();
    fs::write(path.join("B/readme.txt"), b"authored B").unwrap();
    let result=service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[path.join("A/readme.txt"),path.join("B/readme.txt")],"name":"Loose","rootId":"authored-root"})).unwrap();
    let job = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let plan: LocalPlan = serde_json::from_value(job["localPlan"].clone()).unwrap();
    assert_eq!(
        plan.groups[0]
            .files
            .iter()
            .map(|f| f.relative.as_str())
            .collect::<Vec<_>>(),
        vec!["readme.txt", "readme (2).txt"]
    );
    assert!(plan.groups[0].directories.is_empty());
    let result=service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"mode":"batch","sources":[path.join("B"),path.join("A")],"rootId":"authored-root","products":[{"source":path.join("A"),"name":"Alpha","category":"Tool","tags":["A"],"sourceUrl":"https://booth.pm/en/items/123"},{"source":path.join("B"),"name":"Beta","category":"Shader","tags":["B"]}]})).unwrap();
    let mut job = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    let products: Vec<_> = strings(&job["productIds"])
        .iter()
        .map(|id| service.catalogue.get("product", id).unwrap().unwrap())
        .collect();
    assert_eq!(products[0]["title"], "Alpha");
    assert_eq!(products[0]["category"], "Tool");
    assert_eq!(products[0]["tags"], json!(["A"]));
    assert_eq!(products[1]["title"], "Beta");
    assert_eq!(products[1]["category"], "Shader");
    assert_eq!(products[1]["tags"], json!(["B"]));
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn local_membership_add_remove_preserves_metadata_originals_and_modified_bytes() {
    let (path, service) = fixture();
    fs::write(path.join("one.txt"), b"authored one").unwrap();
    fs::write(path.join("two.txt"), b"authored two").unwrap();
    let queued = service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[path.join("one.txt")],"name":"Authored local","category":"Tool","rootId":"authored-root"})).unwrap();
    let id = queued["productIds"][0].as_str().unwrap();
    let mut job = service
        .catalogue
        .get("job", queued["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    job["state"] = json!("completed");
    service
        .catalogue
        .put("job", job["id"].as_str().unwrap(), job.clone())
        .unwrap();
    let original = service.catalogue.get("product", id).unwrap().unwrap();
    let first = service.catalogue.related("artifact", id).unwrap()[0].clone();
    let remove = first["fileId"].as_str().unwrap();
    let args = json!({"draftId":uuid::Uuid::new_v4(),"productId":id,"rootId":"authored-root","addSources":[path.join("two.txt")],"removeFileIds":[remove]});
    let result = service.prepare_local_membership(&args).unwrap();
    assert_eq!(
        service.prepare_local_membership(&args).unwrap()["jobId"],
        result["jobId"]
    );
    let mut mutation = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut mutation, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    mutation["state"] = json!("completed");
    service
        .catalogue
        .put("job", mutation["id"].as_str().unwrap(), mutation.clone())
        .unwrap();
    let product = service.catalogue.get("product", id).unwrap().unwrap();
    assert_eq!(product["title"], original["title"]);
    assert_eq!(product["category"], original["category"]);
    assert_eq!(strings(&product["localManifest"]["fileIds"]).len(), 1);
    assert!(!path
        .join("library")
        .join(first["relativePath"].as_str().unwrap())
        .exists());
    assert!(path.join("one.txt").exists());
    assert!(path.join("two.txt").exists());
    let remaining = service.catalogue.related("artifact", id).unwrap()[0].clone();
    let managed = path
        .join("library")
        .join(remaining["relativePath"].as_str().unwrap());
    fs::remove_file(&managed).unwrap();
    // The edited product's sources outlive both jobs.
    service
        .catalogue
        .command("clear_activity", json!({}))
        .unwrap();
    assert!(service.catalogue.list("job").unwrap().is_empty());
    let repair_id = service
        .retry_local_product_locked(id, &service.catalogue.list("job").unwrap())
        .unwrap()
        .unwrap();
    assert_ne!(repair_id, queued["jobId"].as_str().unwrap());
    let mut repair = service.catalogue.get("job", &repair_id).unwrap().unwrap();
    service
        .run_local_import(&mut repair, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    repair["state"] = json!("completed");
    service.catalogue.put("job", &repair_id, repair).unwrap();
    assert!(managed.exists());
    assert_eq!(
        strings(
            &service.catalogue.get("product", id).unwrap().unwrap()["localManifest"]["fileIds"]
        )
        .len(),
        1
    );
    assert!(service
        .catalogue
        .get("remote_file", remove)
        .unwrap()
        .is_none());
    assert!(service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap_err()
        .to_string()
        .contains("local_membership_changed"));
    assert!(service
        .run_local_import(&mut mutation, Path::new("unused"), &AtomicBool::new(false))
        .unwrap_err()
        .to_string()
        .contains("local_membership_changed"));
    fs::write(&managed, b"user edited").unwrap();
    let result=service.prepare_local_membership(&json!({"draftId":uuid::Uuid::new_v4(),"productId":id,"rootId":"authored-root","removeFileIds":[remaining["fileId"]]})).unwrap();
    let mut job = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(fs::read(&managed).unwrap(), b"user edited");
    assert_eq!(
        service.catalogue.get("product", id).unwrap().unwrap()["localManifest"]["fileIds"],
        json!([])
    );
    service
        .catalogue
        .put(
            "product",
            "online",
            json!({"id":"online","provider":"booth","title":"Authored online"}),
        )
        .unwrap();
    assert!(service.prepare_local_membership(&json!({"draftId":uuid::Uuid::new_v4(),"productId":"online","removeFileIds":["anything"]})).unwrap_err().to_string().contains("local_product_required"));
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn new_local_product_shows_when_it_was_added_and_a_price_read_from_its_link() {
    let (path, service) = fixture();
    fs::write(path.join("one.txt"), b"authored one").unwrap();
    fs::write(path.join("two.txt"), b"authored two").unwrap();
    let queued = service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[path.join("one.txt")],"name":"Authored","rootId":"authored-root","sourceUrl":"https://booth.pm/ja/items/900001"})).unwrap();
    let id = queued["productIds"][0].as_str().unwrap();
    let mut job = service
        .catalogue
        .get("job", queued["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    job["state"] = json!("completed");
    service
        .catalogue
        .put("job", job["id"].as_str().unwrap(), job.clone())
        .unwrap();
    let details = |args: Value| service.catalogue.command("product_details", args).unwrap();
    let shown = details(json!({"id":id}));
    let acquired = shown["acquired"].as_str().unwrap().to_owned();
    assert!(
        chrono::NaiveDateTime::parse_from_str(&acquired, "%Y/%m/%d %H:%M").is_ok(),
        "{acquired}"
    );
    assert!(shown["acquiredSortKey"].is_string());
    assert!(shown["price"].is_null() && shown["currentPriceText"].is_null());

    let before = service.catalogue.get("product", id).unwrap().unwrap();
    let link = "https://booth.pm/ja/items/900001";
    assert!(service
        .store_listing_price(id, link, &json!({"price":1800.0,"priceCurrency":"JPY"}))
        .unwrap());
    // Account scope keeps a local product's own facts.
    for args in [json!({"id":id}), json!({"id":id,"accountIds":[]})] {
        let shown = details(args);
        assert_eq!(
            (shown["price"].as_f64(), shown["priceCurrency"].as_str()),
            (Some(1800.0), Some("JPY"))
        );
        assert_eq!(shown["acquired"], acquired.as_str());
    }
    // A read of a link the user has since replaced is dropped.
    assert!(!service
        .store_listing_price(
            id,
            "https://booth.pm/ja/items/900002",
            &json!({"price":1.0,"priceCurrency":"JPY"})
        )
        .unwrap());
    assert_eq!(details(json!({"id":id}))["price"].as_f64(), Some(1800.0));
    // It is read once, and a retry of the import keeps what the first run recorded.
    let priced = service.catalogue.get("product", id).unwrap().unwrap();
    assert_eq!(
        listing_to_read(&before),
        Some("https://booth.pm/ja/items/900001")
    );
    assert_eq!(listing_to_read(&priced), None);
    let mut rerun = job.clone();
    rerun["completedGroupIndexes"] = json!([]);
    rerun["createdAt"] = json!("2001-01-01T00:00:00+00:00");
    service
        .run_local_import(&mut rerun, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    let kept = service.catalogue.get("product", id).unwrap().unwrap();
    assert_eq!(kept["acquired"], acquired.as_str());
    assert_eq!(kept["price"].as_f64(), Some(1800.0));
    assert_eq!(kept["priceObservedAt"], priced["priceObservedAt"]);
    assert_eq!(kept["priceSourceUrl"], link);
    // A product imported before dates existed stays unknown through a retry too.
    let mut older = kept.clone();
    older["acquired"] = json!("");
    older.as_object_mut().unwrap().remove("priceObservedAt");
    older["price"] = Value::Null;
    older["priceCurrency"] = Value::Null;
    service.catalogue.put("product", id, older.clone()).unwrap();
    assert_eq!(listing_to_read(&older), None);
    service
        .run_local_import(
            &mut rerun.clone(),
            Path::new("unused"),
            &AtomicBool::new(false),
        )
        .unwrap();
    let kept_old = service.catalogue.get("product", id).unwrap().unwrap();
    assert_eq!(kept_old["acquired"], "");
    assert_eq!(listing_to_read(&kept_old), None);
    service.catalogue.put("product", id, kept).unwrap();

    // Adding a file keeps both facts.
    let result = service.prepare_local_membership(&json!({"draftId":uuid::Uuid::new_v4(),"productId":id,"rootId":"authored-root","addSources":[path.join("two.txt")]})).unwrap();
    let mut mutation = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut mutation, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    let shown = details(json!({"id":id}));
    assert_eq!(
        (shown["acquired"].as_str(), shown["price"].as_f64()),
        (Some(acquired.as_str()), Some(1800.0))
    );

    // A store that quotes only text keeps it.
    assert!(service
            .store_listing_price(id, link, &json!({"price":null,"priceCurrency":null,"priceFormatted":"¥1,000","priceSourceCurrency":"JPY"}))
            .unwrap());
    let shown = details(json!({"id":id}));
    assert!(shown["price"].is_null());
    assert_eq!(shown["currentPriceText"], "¥1,000");

    // A link changed in Edit shows no price until its own read arrives, which replaces all
    // of the old quote. An undo goes back to the same state for the earlier link.
    let edit = |url: &str| {
        service
            .catalogue
            .command(
                "overlay",
                json!({"changes":[{"id":id,"patch":{"sourceUrl":url},"clearFields":[]}]}),
            )
            .unwrap()
    };
    let other = "https://jinxxy.com/authored/other";
    let edited = edit(other);
    assert_eq!(edited["changedLinks"], json!([id]));
    assert_eq!(service.current_link(id).as_deref(), Some(other));
    let shown = details(json!({"id":id}));
    assert!(shown["price"].is_null() && shown["currentPriceText"].is_null());
    assert!(!service
        .store_listing_price(id, link, &json!({"price":1.0,"priceCurrency":"JPY"}))
        .unwrap());
    assert!(service
        .store_listing_price(id, other, &json!({"price":50.0,"priceCurrency":"USD"}))
        .unwrap());
    let shown = details(json!({"id":id}));
    assert_eq!(
        (shown["price"].as_f64(), shown["priceCurrency"].as_str()),
        (Some(50.0), Some("USD"))
    );
    assert_eq!(shown["currentPriceText"], "USD 50");
    let record = service.catalogue.get("product", id).unwrap().unwrap();
    assert!(record["priceFormatted"].is_null() && record["priceSourceCurrency"].is_null());
    let undone = service
        .catalogue
        .command("undo", json!({"token":edited["token"]}))
        .unwrap();
    assert_eq!(undone["changedLinks"], json!([id]));
    assert_eq!(service.current_link(id).as_deref(), Some(link));
    assert!(details(json!({"id":id}))["price"].is_null());
    assert!(service
        .store_listing_price(id, link, &json!({"price":1800.0,"priceCurrency":"JPY"}))
        .unwrap());
    assert_eq!(details(json!({"id":id}))["price"].as_f64(), Some(1800.0));
    // A price belongs to the listing, whatever a browser added to the link. A query the
    // catalogue will not carry between devices stays out of the record too.
    let carried = format!("{link}?expires=1#top");
    assert_eq!(edit(&carried)["changedLinks"], json!([id]));
    assert_eq!(details(json!({"id":id}))["price"].as_f64(), Some(1800.0));
    assert!(service
        .store_listing_price(id, &carried, &json!({"price":1700.0,"priceCurrency":"JPY"}))
        .unwrap());
    let record = service.catalogue.get("product", id).unwrap().unwrap();
    assert_eq!(record["priceSourceUrl"], link);
    assert_eq!(details(json!({"id":id}))["price"].as_f64(), Some(1700.0));
    // Only a change asks for a read, and a cleared link has nothing to read.
    assert_eq!(edit(&carried)["changed"], 0);
    assert!(service
        .catalogue
        .command(
            "overlay",
            json!({"changes":[{"id":id,"patch":{"note":"Authored"},"clearFields":[]}]})
        )
        .unwrap()["changedLinks"]
        .as_array()
        .unwrap()
        .is_empty());
    edit("");
    assert_eq!(service.current_link(id), None);
    assert!(details(json!({"id":id}))["price"].is_null());
    // A store product's link keeps its own store prices.
    service.catalogue.put("product", "900001", json!({"id":"900001","store":"BOOTH","provider":"booth","title":"Authored online","sourceTitle":"Authored online","sourceUrl":link})).unwrap();
    assert_eq!(service.current_link("900001"), None);
    assert!(!service
        .store_listing_price("900001", link, &json!({"price":1.0,"priceCurrency":"JPY"}))
        .unwrap());

    // The date is the job's, so a retry does not move it.
    let date = added_date(&json!({"createdAt":"2026-11-05T12:00:00+00:00"}));
    assert!(date.starts_with("2026/11/0"), "{date}");
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn an_import_orders_by_the_clock_its_minute_shows() {
    let job = json!({"createdAt":"2026-11-23T11:00:07.123Z"});
    let first = chrono::DateTime::parse_from_rfc3339(&added_at(&job, 0)).unwrap();
    // This device's clock, like the minute shown and the receipt dates it sorts among.
    assert_eq!(
        first.naive_local().format("%Y/%m/%d %H:%M").to_string(),
        added_date(&job)
    );
    let second = chrono::DateTime::parse_from_rfc3339(&added_at(&job, 1)).unwrap();
    assert_eq!(second - first, chrono::Duration::milliseconds(1));
}

#[test]
fn store_product_accepts_added_files_without_changing_its_store_record() {
    let (path, service) = fixture();
    fs::write(
        path.join("Product_1.0.0.unitypackage"),
        b"authored old version",
    )
    .unwrap();
    let store = json!({"id":"900001","store":"BOOTH","provider":"booth","title":"Authored online","sourceTitle":"Authored online"});
    service
        .catalogue
        .put("product", "900001", store.clone())
        .unwrap();
    service.catalogue.put("entitlement", "grant", json!({"id":"grant","productId":"900001","accountId":"authored-account","fileIds":["authored-account:700001:"],"manifestComplete":true})).unwrap();
    let result = service.prepare_local_membership(&json!({"draftId":uuid::Uuid::new_v4(),"productId":"900001","rootId":"authored-root","addSources":[path.join("Product_1.0.0.unitypackage")]})).unwrap();
    let mut job = service
        .catalogue
        .get("job", result["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        service.catalogue.get("product", "900001").unwrap(),
        Some(store)
    );
    let artifact = service
        .catalogue
        .related("artifact", "900001")
        .unwrap()
        .remove(0);
    assert!(path
        .join("library")
        .join(artifact["relativePath"].as_str().unwrap())
        .exists());
    assert!(path.join("Product_1.0.0.unitypackage").exists());
    let details = service
        .catalogue
        .command("product_details", json!({"id":"900001"}))
        .unwrap();
    let added = details["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["userAdded"] == true)
        .unwrap()
        .clone();
    assert_eq!(
        (added["name"].clone(), added["status"].clone()),
        (json!("Product_1.0.0.unitypackage"), json!("ready"))
    );
    // Downloads still follow only the store's own list.
    assert!(service.remote_files("900001").unwrap().0.is_empty());
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn initial_category_distinguishes_default_from_explicit_choices_and_preserves_overlays() {
    let mut default_overlay = json!({"id":"local:synthetic","note":"keep"});
    apply_initial_category(&mut default_overlay, &json!({"category":"NonVRChat"}));
    assert!(default_overlay.get("category").is_none());
    assert_eq!(default_overlay["note"], "keep");

    for metadata in [
        json!({"category":"NonVRChat","categoryManual":true}),
        json!({"category":"Tool"}),
    ] {
        let mut overlay = json!({"id":"local:synthetic"});
        apply_initial_category(&mut overlay, &metadata);
        assert_eq!(overlay["category"], metadata["category"]);
        overlay["category"] = json!("Shader");
        apply_initial_category(&mut overlay, &metadata);
        assert_eq!(overlay["category"], "Shader");
    }

    let mut automatic_overlay = json!({"id":"local:synthetic"});
    apply_initial_category(
        &mut automatic_overlay,
        &json!({"category":"Tool","categoryManual":false}),
    );
    assert!(automatic_overlay.get("category").is_none());
}

#[cfg(windows)]
#[test]
fn local_import_reads_through_junctioned_parents_but_refuses_links_inside_the_selection() {
    use std::os::windows::process::CommandExt;
    let junction = |link: &Path, target: &Path| {
        let output = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let (path, service) = fixture();
    let documents = path.join("redirected-documents");
    fs::create_dir_all(documents.join("Outfit")).unwrap();
    fs::write(
        documents.join("Outfit/model.unitypackage"),
        b"authored outfit",
    )
    .unwrap();
    // Like a Documents folder redirected elsewhere.
    let link = path.join("Documents");
    junction(&link, &documents);
    let args = |sources: Value| json!({"draftId":uuid::Uuid::new_v4(),"sources":sources,"name":"Outfit","rootId":"authored-root"});
    let queued = service
        .prepare_local_import(&args(json!([link.join("Outfit")])))
        .unwrap();
    let mut job = service
        .catalogue
        .get("job", queued["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    let id = queued["productIds"][0].as_str().unwrap();
    assert_eq!(
        service.catalogue.related("artifact", id).unwrap()[0]["status"],
        "ready"
    );
    // A link inside the selection, or a selected link itself, could lead out of it.
    let outside = path.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("private.txt"), b"never selected").unwrap();
    let escape = documents.join("Outfit").join("escape");
    junction(&escape, &outside);
    for sources in [json!([link.join("Outfit")]), json!([link])] {
        let error = service.prepare_local_import(&args(sources)).unwrap_err();
        assert_eq!(local_error(&error), "unsafe_link");
    }
    fs::remove_dir(&escape).unwrap();
    fs::remove_dir(&link).unwrap();
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[cfg(windows)]
#[test]
fn local_import_waits_out_a_brief_source_lock_and_names_a_lasting_one() {
    use std::os::windows::fs::OpenOptionsExt;
    let (path, service) = fixture();
    let source = path.join("held.unitypackage");
    fs::write(&source, b"authored held payload").unwrap();
    let queued = service.prepare_local_import(&json!({"draftId":uuid::Uuid::new_v4(),"sources":[source],"name":"Held","rootId":"authored-root"})).unwrap();
    let mut job = service
        .catalogue
        .get("job", queued["jobId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    // As a program still writing the file does: an open handle that shares nothing.
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(held);
    });
    service
        .run_local_import(&mut job, Path::new("unused"), &AtomicBool::new(false))
        .unwrap();
    release.join().unwrap();
    let id = queued["productIds"][0].as_str().unwrap();
    assert_eq!(
        service.catalogue.related("artifact", id).unwrap()[0]["status"],
        "ready"
    );
    // A lock that outlasts the wait is named, not a generic failure.
    for code in [32, 33] {
        let lasting = source_error(std::io::Error::from_raw_os_error(code));
        assert_eq!(local_error(&lasting), "storage_source_locked");
    }
    drop(service);
    fs::remove_dir_all(path).unwrap();
}

#[cfg(windows)]
#[test]
fn an_interrupted_import_keeps_an_extraction_made_under_the_earlier_longer_name() {
    // 0.4.0 kept 64 characters of a name. An import an update interrupted after it extracted,
    // and before it recorded the extraction, resumes under a build that keeps fewer.
    let (path, service) = fixture();
    let root = path.join("library");
    let prefix = format!(
        "assets/{}",
        io::readable_component("Authored local", "local:authored")
    );
    let stem = "衣".repeat(40);
    let source = root.join(&prefix).join(format!("{stem}.zip"));
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    let payload = path.join("payload/model.unitypackage");
    fs::create_dir_all(payload.parent().unwrap()).unwrap();
    fs::write(&payload, b"authored package").unwrap();
    let tool = native_seven_zip_test_tool();
    let made = std::process::Command::new(&tool)
        .args(["a", "-tzip", "-y", "-bd"])
        .arg(&source)
        .arg(&payload)
        .output()
        .unwrap();
    assert!(made.status.success());
    let identity = source.to_str().unwrap();
    let earlier = format!(
        "{prefix}/extracted/{}",
        io::readable_component_of(&stem, identity, io::EARLIER_READABLE_UNITS)
    );
    let current = format!(
        "{prefix}/extracted/{}",
        io::readable_component(&stem, identity)
    );
    assert_ne!(earlier, current);
    fs::create_dir_all(root.join(&earlier)).unwrap();
    fs::write(
        root.join(&earlier).join("model.unitypackage"),
        b"authored package",
    )
    .unwrap();
    let group = LocalGroup {
        product_id: "local:authored".into(),
        title: "Authored local".into(),
        prefix,
        files: vec![],
        directories: vec![],
        metadata: None,
        remove_file_ids: vec![],
        membership: false,
        expected_mutation: None,
    };
    let (relative, inventory, _) = service
        .extract_local_archive(
            &root,
            &group,
            &source,
            std::slice::from_ref(&source),
            None,
            &[],
            &tool,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(relative, earlier);
    assert_eq!(inventory.as_array().unwrap().len(), 1);
    assert!(!root.join(&current).exists());
    drop(service);
    fs::remove_dir_all(path).unwrap();
}
