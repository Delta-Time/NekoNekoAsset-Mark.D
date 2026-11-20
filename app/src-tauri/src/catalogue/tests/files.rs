//! Owned files: manifests, downloads, readiness and files a store dropped.
use super::*;

#[test]
fn reopen_corrects_stale_partial_badge_without_store_sync() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "product",
        "p",
        json!({"id":"p","sourceTitle":"Authored product"}),
    )
    .unwrap();
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"p","fileIds":["f"],"manifestComplete":false}),
    )
    .unwrap();
    c.put(
        "remote_file",
        "f",
        json!({"id":"f","productId":"p","name":"authored.unitypackage","revision":"r1"}),
    )
    .unwrap();
    c.put(
        "artifact",
        "a",
        json!({"id":"a","productId":"p","fileId":"f","revision":"r1","status":"ready"}),
    )
    .unwrap();
    assert_eq!(projection(&c, "p")["status"], "ready");
    {
        let conn = c.inner.lock().unwrap();
        conn.execute("UPDATE products SET status='partial',downloaded=0,body=json_set(body,'$.status','partial','$.fullyDownloaded',json('false')),summary=json_set(summary,'$.status','partial','$.fullyDownloaded',json('false')) WHERE id='p'", []).unwrap();
    }
    drop(c);
    let reopened = fixture.open();
    let product = projection(&reopened, "p");
    assert_eq!(product["manifestComplete"], false);
    assert_eq!(product["status"], "ready");
    assert_eq!(product["fullyDownloaded"], true);
    assert_eq!(
        reopened
            .command("query", json!({"quick":"downloaded","limit":10}))
            .unwrap()["allDownloaded"],
        true
    );
}

#[test]
fn store_dropped_downloads_and_added_files_stay_listed_while_their_bytes_do() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let source = json!({"id":"p","store":"BOOTH","sourceTitle":"Authored product"});
    c.put("product", "p", source.clone()).unwrap();
    // The store replaced old.zip, already downloaded but its extraction failed,
    // with new.zip, and dropped never.zip, which was never downloaded. A later
    // Full sync had flagged old.zip for content verification before it went.
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"p","fileIds":["acct:700001:800001"],"manifestComplete":true}),
    )
    .unwrap();
    for (id, source_file, name) in [
        ("acct:700001:800001", "700001", "new.zip"),
        ("acct:700000:800001", "700000", "old.zip"),
        ("acct:700002:800001", "700002", "never.zip"),
    ] {
        c.put("remote_file", id, json!({"id":id,"productId":"p","accountId":"acct","sourceFileId":source_file,"name":name,"revision":"r1","requiresContentVerification":source_file == "700000"})).unwrap();
    }
    c.put("artifact", "a-old", json!({"id":"a-old","productId":"p","fileId":"acct:700000:800001","revision":"r1","status":"extractionFailed","errorCode":"archive_password_required","originalAvailable":true})).unwrap();
    // A downloaded variantless descriptor of the downloadable the grant still offers.
    c.put("remote_file", "acct:700001:", json!({"id":"acct:700001:","productId":"p","accountId":"acct","sourceFileId":"700001","name":"new.zip","revision":"r1"})).unwrap();
    c.put("artifact", "a-alias", json!({"id":"a-alias","productId":"p","fileId":"acct:700001:","revision":"r1","status":"ready","originalAvailable":true})).unwrap();
    c.put("remote_file", "local-file:added", json!({"id":"local-file:added","productId":"p","provider":"local","originKind":"local","name":"added.zip","revision":"sha256:a"})).unwrap();
    c.put("artifact", "a-added", json!({"id":"a-added","productId":"p","fileId":"local-file:added","revision":"sha256:a","status":"ready","originalAvailable":true})).unwrap();
    let product = projection(&c, "p");
    let files = product["files"].as_array().unwrap();
    let ids: Vec<_> = files.iter().map(|f| text(f, "id")).collect();
    assert_eq!(
        ids,
        [
            "acct:700001:800001",
            "acct:700000:800001",
            "local-file:added"
        ]
    );
    assert_eq!(files[0]["status"], "notDownloaded");
    assert!(files[0].get("kept").is_none());
    // No store revision remains to verify or retry: the copy stays usable, not Attention.
    assert_eq!(
        (files[1]["kept"].clone(), files[1]["status"].clone()),
        (json!(true), json!("downloaded"))
    );
    assert!(files[1].get("userAdded").is_none() && files[1].get("errorCode").is_none());
    assert_eq!(
        (
            files[2]["kept"].clone(),
            files[2]["userAdded"].clone(),
            files[2]["status"].clone()
        ),
        (json!(true), json!(true), json!("ready"))
    );
    assert_eq!(
        (
            product["total"].clone(),
            product["downloaded"].clone(),
            product["keptFiles"].clone()
        ),
        (json!(3), json!(2), json!(2))
    );
    assert_eq!(
        (
            product["status"].clone(),
            product["manifestComplete"].clone()
        ),
        (json!("partial"), json!(true))
    );
    // A partly deleted kept file stays listed so its own delete can finish; it
    // is Attention and no longer counts as downloaded.
    let mut partial = c.get("artifact", "a-old").unwrap().unwrap();
    partial["status"] = json!("missing");
    partial["errorCode"] = json!("download_delete_incomplete");
    c.put("artifact", "a-old", partial).unwrap();
    let product = projection(&c, "p");
    assert_eq!(product["files"][1]["status"], "missing");
    assert_eq!(
        (
            product["downloaded"].clone(),
            product["keptFiles"].clone(),
            product["status"].clone()
        ),
        (json!(1), json!(1), json!("attention"))
    );
    // Deleting a kept file's record removes its row and its search terms.
    let search = |term: &str| {
        let category = text(&projection(&c, "p"), "category").to_owned();
        c.command(
            "query",
            json!({"search":term,"category":category,"includeFiles":false}),
        )
        .unwrap()["matched"]
            .clone()
    };
    assert_eq!(search("old.zip"), 1);
    c.delete("artifact", "a-old").unwrap();
    let product = projection(&c, "p");
    assert_eq!(
        product["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| text(f, "id"))
            .collect::<Vec<_>>(),
        ["acct:700001:800001", "local-file:added"]
    );
    assert_eq!(
        (
            product["total"].clone(),
            product["knownFileCount"].clone(),
            product["keptFiles"].clone()
        ),
        (json!(2), json!(2), json!(1))
    );
    assert_eq!(search("old.zip"), 0);
    c.put("product", "p", source).unwrap();
    let rebuilt = projection(&c, "p");
    assert_eq!(
        (rebuilt["total"].clone(), rebuilt["keptFiles"].clone()),
        (json!(2), json!(1))
    );
    // Without any grant, an added file still never joins the store manifest.
    c.delete("entitlement", "e").unwrap();
    let ungranted = projection(&c, "p");
    let added = ungranted["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "local-file:added")
        .unwrap()
        .clone();
    assert_eq!(
        (added["kept"].clone(), added["userAdded"].clone()),
        (json!(true), json!(true))
    );
}

#[test]
fn reopen_lists_downloads_dropped_before_kept_files_existed() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "product",
        "p",
        json!({"id":"p","store":"BOOTH","sourceTitle":"Authored product"}),
    )
    .unwrap();
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"p","fileIds":["new"],"manifestComplete":true}),
    )
    .unwrap();
    for id in ["new", "old"] {
        c.put(
            "remote_file",
            id,
            json!({"id":id,"productId":"p","name":format!("{id}.zip"),"revision":"r1"}),
        )
        .unwrap();
    }
    c.put("artifact", "a-old", json!({"id":"a-old","productId":"p","fileId":"old","revision":"r1","status":"ready","originalAvailable":true})).unwrap();
    {
        // The projection an earlier version stored: only the grant's file.
        let conn = c.inner.lock().unwrap();
        conn.execute("UPDATE products SET body=json_set(json_remove(body,'$.files[1]','$.keptFiles'),'$.total',1,'$.knownFileCount',1),summary=json_set(json_remove(summary,'$.keptFiles'),'$.total',1,'$.knownFileCount',1) WHERE id='p'", []).unwrap();
    }
    assert_eq!(projection(&c, "p")["files"].as_array().unwrap().len(), 1);
    drop(c);
    let reopened = fixture.open();
    let product = projection(&reopened, "p");
    assert_eq!(
        (
            product["keptFiles"].clone(),
            product["files"][1]["id"].clone()
        ),
        (json!(1), json!("old"))
    );
    // Later opens find nothing left to list.
    let revisions = reopened.change_domains().unwrap();
    drop(reopened);
    assert_eq!(fixture.open().change_domains().unwrap(), revisions);
}

#[test]
fn a_refused_file_leaves_the_list_and_search_and_reopen_drops_one_listed_before() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "product",
        "p",
        json!({"id":"p","store":"BOOTH","sourceTitle":"Authored product"}),
    )
    .unwrap();
    // A grant still names the file its store now refuses, as an update can leave it.
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"p","fileIds":["kept","gone"],"manifestComplete":true}),
    )
    .unwrap();
    for id in ["kept", "gone"] {
        c.put(
            "remote_file",
            id,
            json!({"id":id,"productId":"p","name":format!("{id}.zip"),"revision":"r1"}),
        )
        .unwrap();
    }
    c.put("artifact", "a-kept", json!({"id":"a-kept","productId":"p","fileId":"kept","revision":"r1","status":"ready","originalAvailable":true})).unwrap();
    let category = text(&projection(&c, "p"), "category").to_owned();
    let found = |c: &Catalogue, term: &str| {
        c.command(
            "query",
            json!({"search":term,"category":category,"includeFiles":false}),
        )
        .unwrap()["matched"]
            .clone()
    };
    assert_eq!(found(&c, "gone.zip"), 1);
    c.put("job", "refused", json!({"id":"refused","productId":"p","type":"download","state":"needsAction","updatedAt":"2026-01-02T00:00:00Z","errorCode":"store_file_unavailable","failures":{"gone":{"fileId":"gone","errorCode":"store_file_unavailable"}}})).unwrap();
    c.put("artifact", "pending-gone", json!({"id":"pending-gone","productId":"p","fileId":"gone","revision":null,"status":"notDownloaded","errorCode":"store_file_unavailable","originalAvailable":false})).unwrap();
    let product = projection(&c, "p");
    assert_eq!(
        product["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| text(f, "id"))
            .collect::<Vec<_>>(),
        ["kept"]
    );
    assert_eq!(
        (product["total"].clone(), product["status"].clone()),
        (json!(1), json!("ready"))
    );
    assert_eq!(found(&c, "gone.zip"), 0);
    {
        // The projection an earlier version stored: the refused file still listed.
        let conn = c.inner.lock().unwrap();
        conn.execute("UPDATE products SET body=json_set(json_insert(body,'$.files[#]',json('{\"id\":\"gone\",\"name\":\"gone.zip\",\"status\":\"unknown\"}')),'$.total',2,'$.status','attention'),status='attention' WHERE id='p'", []).unwrap();
    }
    assert_eq!(projection(&c, "p")["files"].as_array().unwrap().len(), 2);
    drop(c);
    let reopened = fixture.open();
    let product = projection(&reopened, "p");
    assert_eq!(
        (
            product["files"].as_array().unwrap().len(),
            product["status"].clone()
        ),
        (1, json!("ready"))
    );
    // Later opens find nothing left to drop.
    let revisions = reopened.change_domains().unwrap();
    drop(reopened);
    assert_eq!(fixture.open().change_domains().unwrap(), revisions);
}

#[test]
fn current_revision_and_incomplete_manifest_never_ready() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.put(
        "remote_file",
        "f",
        json!({"id":"f","productId":"1","accountId":"a","name":"test.zip","revision":"r1"}),
    )
    .unwrap();
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"1","accountId":"a","fileIds":["f"],"manifestComplete":true}),
    )
    .unwrap();
    c.put("artifact","a",json!({"id":"a","productId":"1","fileId":"f","revision":"r1","status":"ready","rootId":"r","relativePath":"product/test.zip","originalAvailable":false})).unwrap();
    assert_eq!(projection(&c, "1")["status"], "ready");
    assert_eq!(projection(&c, "1")["files"][0]["originalAvailable"], false);
    c.put(
        "remote_file",
        "f",
        json!({"id":"f","productId":"1","accountId":"a","name":"test.zip","revision":"r2"}),
    )
    .unwrap();
    assert_eq!(projection(&c, "1")["status"], "notDownloaded");
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"1","accountId":"a","fileIds":["f"],"manifestComplete":false}),
    )
    .unwrap();
    assert_eq!(projection(&c, "1")["status"], "notDownloaded");
}

#[test]
fn partial_twenty_nine_of_thirty_without_failure_is_partial() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    let mut files = vec![];
    for n in 0..30 {
        let id = format!("f{n}");
        files.push(id.clone());
        c.put("remote_file",&id,json!({"id":id,"productId":"1","name":format!("file{n}.unitypackage"),"revision":"one"})).unwrap();
        if n < 29 {
            c.put("artifact",&id,json!({"id":id,"productId":"1","fileId":id,"revision":"one","status":"ready","relativePath":format!("files/{n}.unitypackage")})).unwrap();
        }
    }
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"1","fileIds":files,"manifestComplete":true}),
    )
    .unwrap();
    let p = projection(&c, "1");
    assert_eq!(p["downloaded"], 29);
    assert_eq!(p["total"], 30);
    assert_eq!(p["status"], "partial");
}

#[test]
fn local_manifest_and_batch_jobs_preserve_required_files_without_accounts() {
    let f = Fixture::new();
    let c = f.open();
    c.put("root", "local-root", json!({"id":"local-root","label":"Synthetic local root","path":"X:/Synthetic local library","status":"online"})).unwrap();
    let local = |id: &str, files: Value| json!({"id":id,"provider":"local","store":"Local Files","custom":true,"title":"Synthetic local Hair","sourceCategory":{"name":"Hair"},"localManifest":{"complete":true,"fileIds":files,"directories":["package/empty/nested"]},"importedAt":"2026-10-10T00:00:00Z"});
    c.put(
        "product",
        "local:one",
        local("local:one", json!(["local:file-a", "local:file-b"])),
    )
    .unwrap();
    c.put(
        "product",
        "local:two",
        local("local:two", json!(["local:file-c"])),
    )
    .unwrap();
    c.put("acquisition", "local-import-fingerprint:synthetic", json!({"id":"local-import-fingerprint:synthetic","productId":"local:one","observationKind":"localImport","provider":"local","source":"local","importedAt":"2026-10-10T00:00:00Z"})).unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["manifestComplete"], false);
    for (file, product) in [
        ("local:file-a", "local:one"),
        ("local:file-b", "local:one"),
        ("local:file-c", "local:two"),
    ] {
        c.put("remote_file",file,json!({"id":file,"productId":product,"provider":"local","name":"Synthetic.unitypackage","revision":"r1","sizeBytes":4})).unwrap();
    }
    let pending = dynamic_details(&c, "local:one");
    assert_eq!(pending["manifestComplete"], true);
    assert_eq!(pending["status"], "notDownloaded");
    assert_eq!(pending["accountIds"], json!([]));
    assert_eq!(pending["acquired"], "");
    assert!(pending["price"].is_null());
    assert!(pending["paidPrice"].is_null());
    assert_eq!(pending["sources"][0]["provider"], "Local Files");
    let mut job = json!({"id":"local-import:synthetic","type":"storage:import_local","productIds":["local:one","local:two"],"completedProductIds":[],"state":"running","progress":1});
    c.put("job", "local-import:synthetic", job.clone()).unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "processing");
    assert_eq!(dynamic_details(&c, "local:two")["status"], "processing");
    let before = c.change_domains().unwrap();
    job["progress"] = json!(99);
    c.put("job", "local-import:synthetic", job.clone()).unwrap();
    let after = c.change_domains().unwrap();
    assert!(after.0 > before.0);
    assert_eq!(after.1, before.1);
    let artifact = |file: &str, product: &str, status: &str| json!({"id":format!("artifact:{file}"),"fileId":file,"productId":product,"rootId":"local-root","revision":"r1","status":status,"hash":"synthetic-hash","sizeBytes":4,"relativePath":format!("package/{}",file.replace(':',"-")),"originalAvailable":true});
    c.put(
        "artifact",
        "artifact:local:file-a",
        artifact("local:file-a", "local:one", "ready"),
    )
    .unwrap();
    job["state"] = json!("cancelled");
    c.put("job", "local-import:synthetic", job.clone()).unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "partial");
    assert_eq!(dynamic_details(&c, "local:one")["fullyDownloaded"], false);
    assert_eq!(dynamic_details(&c, "local:one")["downloaded"], 1);
    c.put(
        "artifact",
        "artifact:local:file-b",
        artifact("local:file-b", "local:one", "extractionFailed"),
    )
    .unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "attention");
    c.put(
        "artifact",
        "artifact:local:file-b",
        artifact("local:file-b", "local:one", "ready"),
    )
    .unwrap();
    job["state"] = json!("running");
    job["completedProductIds"] = json!(["local:one"]);
    c.put_many(vec![(
        "job".into(),
        "local-import:synthetic".into(),
        job.clone(),
    )])
    .unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "ready");
    assert_eq!(dynamic_details(&c, "local:two")["status"], "processing");
    job["state"] = json!("failed");
    job["errorCode"] = json!("storage.file_missing");
    c.put("job", "local-import:synthetic", job).unwrap();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "ready");
    assert_eq!(dynamic_details(&c, "local:two")["status"], "attention");
    // One import holds both products: dismissing one leaves the job's own facts alone.
    c.command("dismiss_attention", json!({"productIds":["local:two"]}))
        .unwrap();
    assert_eq!(dynamic_details(&c, "local:two")["status"], "notDownloaded");
    assert_eq!(
        c.get("job", "local-import:synthetic").unwrap().unwrap()["state"],
        "failed"
    );
    assert_eq!(
        c.command("query", json!({"accountIds":["unrelated"]}))
            .unwrap()["matched"],
        0
    );
    assert!(c.list("account").unwrap().is_empty());
    let manifest = c.manifest().unwrap();
    drop(c);
    let c = f.open();
    assert_eq!(dynamic_details(&c, "local:one")["status"], "ready");
    let fresh = Fixture::new();
    let other = fresh.open();
    other.restore_manifest(manifest).unwrap();
    let restored = dynamic_details(&other, "local:one");
    assert_eq!(
        restored["localManifest"]["directories"],
        json!(["package/empty/nested"])
    );
    // Metadata restore requires root mapping and storage verification before Ready.
    assert_eq!(restored["status"], "notDownloaded");
    assert_eq!(
        other.get("root", "local-root").unwrap().unwrap()["status"],
        "needsMapping"
    );
    other.put("root", "local-root", json!({"id":"local-root","label":"Synthetic mapped root","path":"Y:/Synthetic restored library","status":"online"})).unwrap();
    assert_eq!(
        dynamic_details(&other, "local:one")["status"],
        "notDownloaded"
    );
    // Model the verified facts that Storage publishes after checking copied files.
    for file in ["local:file-a", "local:file-b"] {
        other
            .put(
                "artifact",
                &format!("artifact:{file}"),
                artifact(file, "local:one", "ready"),
            )
            .unwrap();
    }
    assert_eq!(dynamic_details(&other, "local:one")["status"], "ready");
    assert_eq!(restored["acquired"], "");
    assert!(restored["paidPrice"].is_null());
    assert!(other.list("account").unwrap().is_empty());
    assert!(other.list("job").unwrap().is_empty());
    let invalid = local("local:empty", json!([]));
    assert!(other.put("product", "local:empty", invalid).is_err());
}

#[test]
fn required_manifest_membership_includes_every_current_grant() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.put(
        "remote_file",
        "f",
        json!({"id":"f","productId":"1","accountId":"a","name":"test.zip","revision":"r"}),
    )
    .unwrap();
    c.put(
        "entitlement",
        "a",
        json!({"id":"a","productId":"1","accountId":"a","fileIds":["f"],"manifestComplete":true}),
    )
    .unwrap();
    c.put(
        "entitlement",
        "b",
        json!({"id":"b","productId":"1","accountId":"b","fileIds":[],"manifestComplete":false}),
    )
    .unwrap();
    assert_eq!(c.effective_entitlements("1").unwrap().len(), 2);
    assert_eq!(projection(&c, "1")["manifestComplete"], false);
    c.put(
        "entitlement",
        "b",
        json!({"id":"b","productId":"1","accountId":"b","fileIds":["f"],"manifestComplete":true}),
    )
    .unwrap();
    assert_eq!(projection(&c, "1")["manifestComplete"], true);
}
