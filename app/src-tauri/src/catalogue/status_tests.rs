//! Independent synthetic projection regressions. No filesystem or live metadata.
use super::*;

fn fixture(file_count: usize, complete: bool) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE entities(kind TEXT NOT NULL,id TEXT NOT NULL,product_id TEXT NOT NULL DEFAULT '',body TEXT NOT NULL,PRIMARY KEY(kind,id));").unwrap();
    set_raw(
        &conn,
        "product",
        "p",
        &json!({"id":"p","sourceTitle":"Synthetic product"}),
    )
    .unwrap();
    manifest(&conn, file_count, complete);
    for index in 0..file_count {
        let id = format!("f{index}");
        set_raw(&conn,"remote_file",&id,&json!({"id":id,"productId":"p","name":format!("synthetic-{index}.unitypackage"),"revision":"r1"})).unwrap();
    }
    conn
}

fn manifest(conn: &Connection, file_count: usize, complete: bool) {
    let ids: Vec<_> = (0..file_count).map(|i| format!("f{i}")).collect();
    set_raw(
        conn,
        "entitlement",
        "e",
        &json!({"id":"e","productId":"p","fileIds":ids,"manifestComplete":complete}),
    )
    .unwrap();
}

fn artifact(conn: &Connection, index: usize, status: &str, error: Option<&str>) {
    let id = format!("f{index}");
    set_raw(conn,"artifact",&id,&json!({"id":id,"productId":"p","fileId":id,"revision":"r1","status":status,"errorCode":error})).unwrap();
}

/// The record a file's last download attempt leaves: `downloading` while it runs, then
/// `notDownloaded` with the reason it ended.
fn attempt(conn: &Connection, index: usize, error: Option<&str>) {
    let id = format!("pending-f{index}");
    let status = if error.is_some() {
        "notDownloaded"
    } else {
        "downloading"
    };
    set_raw(conn,"artifact",&id,&json!({"id":id,"productId":"p","fileId":format!("f{index}"),"revision":null,"status":status,"errorCode":error,"originalAvailable":false})).unwrap();
}

fn job(conn: &Connection, id: &str, state: &str, updated: &str, error: Option<&str>) {
    set_raw(conn,"job",id,&json!({"id":id,"productId":"p","type":"download","state":state,"updatedAt":updated,"errorCode":error})).unwrap();
}

fn projected(conn: &Connection) -> Value {
    projection::effective(conn, "p").unwrap().unwrap()
}

#[test]
fn ordinary_unstarted_partial_pending_and_cancelled_work_is_not_attention() {
    let conn = fixture(2, true);
    assert_eq!(projected(&conn)["status"], "notDownloaded");
    manifest(&conn, 2, false);
    assert_eq!(projected(&conn)["status"], "notDownloaded");
    assert_eq!(projected(&conn)["knownFileCount"], 2);
    manifest(&conn, 2, true);
    artifact(&conn, 0, "ready", None);
    for state in ["notDownloaded", "partial", "pending", "cancelled"] {
        artifact(&conn, 1, state, None);
        let product = projected(&conn);
        assert_eq!(product["status"], "partial", "{state}");
        assert_eq!(product["downloaded"], 1);
        assert_eq!(product["total"], 2);
        assert_eq!(product["fullyDownloaded"], false);
    }
    artifact(&conn, 1, "notDownloaded", Some("cancelled"));
    job(&conn, "latest", "cancelled", "2026-01-02T00:00:00Z", None);
    assert_eq!(projected(&conn)["status"], "partial");
    for state in ["queued", "running"] {
        job(&conn, "latest", state, "2026-01-02T00:00:00Z", None);
        assert_eq!(projected(&conn)["status"], "processing");
    }
}

#[test]
fn genuine_file_or_last_attempt_failure_is_attention_and_cancel_does_not_hide_missing() {
    let conn = fixture(1, true);
    for state in [
        "failed",
        "needsAction",
        "missing",
        "corrupt",
        "extractionFailed",
        "attention",
    ] {
        artifact(&conn, 0, state, None);
        assert_eq!(projected(&conn)["status"], "attention", "{state}");
    }
    for state in ["partial", "notDownloaded"] {
        artifact(&conn, 0, state, Some("hash_mismatch"));
        assert_eq!(projected(&conn)["status"], "attention");
    }
    conn.execute("DELETE FROM entities WHERE kind='artifact'", [])
        .unwrap();
    // A finished download job decides nothing, since Clear history removes it; the
    // record its failed attempt left is the file's own fact.
    for state in ["failed", "needsAction"] {
        job(
            &conn,
            "old",
            state,
            "2026-01-01T00:00:00Z",
            Some("network_failed"),
        );
        assert_eq!(projected(&conn)["status"], "notDownloaded");
    }
    attempt(&conn, 0, Some("network_failed"));
    let product = projected(&conn);
    assert_eq!(product["status"], "attention");
    assert_eq!(
        (
            &product["files"][0]["status"],
            &product["files"][0]["errorCode"]
        ),
        (&json!("failed"), &json!("network_failed"))
    );
    // A new attempt replaces the failure. A cancelled one is no fault, nor is one that
    // closing the App interrupted (its job resumes at the next start).
    for later in [None, Some("cancelled"), Some("interrupted")] {
        attempt(&conn, 0, later);
        assert_eq!(projected(&conn)["status"], "notDownloaded", "{later:?}");
    }
    artifact(&conn, 0, "missing", None);
    assert_eq!(projected(&conn)["status"], "attention");
}

#[test]
fn dismissing_attention_keeps_file_facts_and_the_next_failure_raises_it_again() {
    fn dismiss(conn: &Connection) {
        for (kind, record) in projection::dismissed_attention(conn, "p", &[]).unwrap() {
            set_raw(conn, kind, text(&record, "id"), &record).unwrap();
        }
    }
    let conn = fixture(1, true);
    artifact(
        &conn,
        0,
        "extractionFailed",
        Some("archive_password_required"),
    );
    attempt(&conn, 0, Some("store_access_denied"));
    assert_eq!(projected(&conn)["status"], "attention");
    dismiss(&conn);
    let product = projected(&conn);
    assert_eq!(product["status"], "partial");
    assert_eq!(product["files"][0]["status"], "extractionFailed");
    assert_eq!(
        product["files"][0]["errorCode"],
        "archive_password_required"
    );
    // Clearing Attention keeps the failure itself, even on a file sync asks to verify.
    let mut file = raw(&conn, "remote_file", "f0").unwrap().unwrap();
    file["requiresContentVerification"] = json!(true);
    set_raw(&conn, "remote_file", "f0", &file).unwrap();
    assert_eq!(projected(&conn)["files"][0]["status"], "extractionFailed");
    file["requiresContentVerification"] = json!(false);
    set_raw(&conn, "remote_file", "f0", &file).unwrap();
    // A new attempt that fails leaves a new record, whatever the earlier one said.
    attempt(&conn, 0, Some("network_failed"));
    assert_eq!(projected(&conn)["status"], "attention");
    dismiss(&conn);
    assert_eq!(projected(&conn)["status"], "partial");
    // A download that fails again replaces the artifact, so the mark goes with it.
    artifact(&conn, 0, "missing", None);
    assert_eq!(projected(&conn)["status"], "attention");
    dismiss(&conn);
    assert_eq!(projected(&conn)["status"], "notDownloaded");
}

#[test]
fn cancelled_extraction_retains_bytes_without_claiming_ready_or_failure() {
    let conn = fixture(1, true);
    artifact(&conn, 0, "extractionFailed", Some("cancelled"));
    job(&conn, "latest", "cancelled", "2026-01-02T00:00:00Z", None);
    let product = projected(&conn);
    assert_eq!(product["status"], "partial");
    assert_eq!(product["files"][0]["status"], "downloaded");
    assert_eq!(product["downloaded"], 1);
    assert_eq!(product["fullyDownloaded"], true);
    artifact(
        &conn,
        0,
        "extractionFailed",
        Some("archive_password_required"),
    );
    assert_eq!(projected(&conn)["status"], "attention");
}

#[test]
fn ready_requires_every_observed_file_but_not_source_closure() {
    let conn = fixture(2, true);
    artifact(&conn, 0, "ready", None);
    artifact(&conn, 1, "ready", None);
    assert_eq!(projected(&conn)["status"], "ready");
    job(
        &conn,
        "old-failure",
        "failed",
        "2026-01-01T00:00:00Z",
        Some("network_failed"),
    );
    assert_eq!(projected(&conn)["status"], "ready");
    job(
        &conn,
        "old-failure",
        "needsAction",
        "2026-01-01T00:00:00Z",
        Some("source_tree_incomplete"),
    );
    assert_eq!(projected(&conn)["status"], "ready");
    job(
        &conn,
        "old-failure",
        "needsAction",
        "2026-01-01T00:00:00Z",
        Some("manifest_incomplete"),
    );
    manifest(&conn, 2, false);
    assert_eq!(projected(&conn)["status"], "ready");
    assert_eq!(projected(&conn)["manifestComplete"], false);
    assert_eq!(projected(&conn)["fullyDownloaded"], true);
    manifest(&conn, 2, true);
    let mut file = raw(&conn, "remote_file", "f1").unwrap().unwrap();
    file["revision"] = json!("r2");
    set_raw(&conn, "remote_file", "f1", &file).unwrap();
    assert_eq!(projected(&conn)["status"], "partial");
    file["revision"] = json!("r1");
    file["requiresContentVerification"] = json!(true);
    set_raw(&conn, "remote_file", "f1", &file).unwrap();
    assert_eq!(projected(&conn)["status"], "partial");
    assert_eq!(projected(&conn)["fullyDownloaded"], false);
    artifact(&conn, 1, "missing", None);
    assert_eq!(projected(&conn)["status"], "attention");
    conn.execute(
        "DELETE FROM entities WHERE kind='remote_file' AND id='f1'",
        [],
    )
    .unwrap();
    assert_eq!(projected(&conn)["status"], "partial");
    assert_eq!(projected(&conn)["manifestComplete"], false);
    assert_eq!(projected(&conn)["knownFileCount"], 1);
    assert_eq!(projected(&conn)["fullyDownloaded"], false);
}

#[test]
fn seven_ready_files_are_downloaded_during_sync_then_new_file_is_partial() {
    let conn = fixture(7, false);
    for index in 0..7 {
        artifact(&conn, index, "ready", None);
    }
    set_raw(
        &conn,
        "job",
        "source-sync",
        &json!({"id":"source-sync","productId":"p","type":"boothSync","state":"running"}),
    )
    .unwrap();
    let product = projected(&conn);
    assert_eq!(product["manifestComplete"], false);
    assert_eq!(product["downloaded"], 7);
    assert_eq!(product["status"], "ready");
    assert_eq!(product["fullyDownloaded"], true);

    manifest(&conn, 8, false);
    set_raw(
        &conn,
        "remote_file",
        "f7",
        &json!({"id":"f7","productId":"p","name":"later-file.unitypackage","revision":"r1"}),
    )
    .unwrap();
    let product = projected(&conn);
    assert_eq!(product["downloaded"], 7);
    assert_eq!(product["total"], 8);
    assert_eq!(product["status"], "partial");
    assert_eq!(product["fullyDownloaded"], false);
}

#[test]
fn twenty_nine_of_thirty_is_incomplete_but_attention_requires_failure() {
    let conn = fixture(30, true);
    for index in 0..29 {
        artifact(&conn, index, "ready", None);
    }
    let product = projected(&conn);
    assert_eq!(product["status"], "partial");
    assert_eq!(product["downloaded"], 29);
    assert_eq!(product["total"], 30);
    assert_eq!(product["fullyDownloaded"], false);
    artifact(&conn, 29, "missing", None);
    assert_eq!(projected(&conn)["status"], "attention");
    artifact(&conn, 29, "ready", None);
    assert_eq!(projected(&conn)["status"], "ready");
    assert_eq!(projected(&conn)["fullyDownloaded"], true);
}

#[test]
fn exact_avatar_tags_add_compatibility_evidence_without_entitlement_and_honor_overlays() {
    let conn = fixture(0, false);
    let source = json!({"id":"p","sourceTitle":"Synthetic product","sourceTags":["Ｍａｆｕｙｕ対応","ordinary"]});
    set_raw(&conn, "product", "p", &source).unwrap();
    let overlay = json!({"id":"p","tags":["真冬 / Mafuyu","Mafuyu coat"]});
    set_raw(&conn, "overlay", "p", &overlay).unwrap();
    let product = projected(&conn);
    assert_eq!(product["ownedAvatars"].as_array().unwrap().len(), 1);
    let evidence = product["ownedAvatarEvidence"].as_array().unwrap();
    assert!(evidence
        .iter()
        .any(|v| v["avatarId"] == "booth:5007531" && v["source"] == "sourceTag"));
    assert!(evidence
        .iter()
        .any(|v| v["avatarId"] == "booth:5007531" && v["source"] == "userTag"));
    assert!(evidence
        .iter()
        .all(|v| v["entitlementEstablished"] == false));
    assert!(!evidence.iter().any(|v| v["tag"] == "Mafuyu coat"));
    assert_eq!(product["manifestComplete"], false);
    assert_eq!(product["fullyDownloaded"], false);
    assert_eq!(product["status"], "notDownloaded");
    assert_eq!(raw(&conn, "product", "p").unwrap().unwrap(), source);
    let entitlement = raw(&conn, "entitlement", "e").unwrap().unwrap();
    assert_eq!(entitlement["fileIds"], json!([]));
    let mut rejected = overlay.clone();
    rejected["rejectedTags"] = json!(["真冬"]);
    set_raw(&conn, "overlay", "p", &rejected).unwrap();
    assert!(projected(&conn)["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut explicit = overlay.clone();
    explicit["ownedAvatars"] = json!([]);
    set_raw(&conn, "overlay", "p", &explicit).unwrap();
    assert!(projected(&conn)["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .is_empty());
    explicit["ownedAvatars"] = json!(["Synthetic Manual"]);
    set_raw(&conn, "overlay", "p", &explicit).unwrap();
    let product = projected(&conn);
    assert_eq!(product["ownedAvatars"], json!(["Synthetic Manual"]));
    assert!(product["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["source"] == "user"));
}

#[test]
fn a_file_its_store_refused_counts_as_dropped_and_raises_no_attention() {
    // The record a download leaves when the store answers that a file is no longer offered.
    fn refuse(conn: &Connection, index: usize) {
        let id = format!("pending-f{index}");
        set_raw(conn,"artifact",&id,&json!({"id":id,"productId":"p","fileId":format!("f{index}"),"revision":null,"status":"notDownloaded","errorCode":"store_file_unavailable","originalAvailable":false})).unwrap();
    }
    fn failed(conn: &Connection, failures: Value) {
        set_raw(conn,"job","latest",&json!({"id":"latest","productId":"p","type":"download","state":"needsAction","updatedAt":"2026-01-02T00:00:00Z","errorCode":"store_file_unavailable","failures":failures})).unwrap();
    }
    fn listed(product: &Value) -> Vec<String> {
        product["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| text(file, "id").to_owned())
            .collect()
    }
    let refusal = json!({"fileId":"f1","errorCode":"store_file_unavailable"});
    let conn = fixture(3, true);
    artifact(&conn, 0, "ready", None);
    refuse(&conn, 1);
    failed(&conn, json!({"f1":refusal}));
    // The grant still names f1, as an update can leave it, but nothing of it was downloaded.
    let product = projected(&conn);
    assert_eq!(listed(&product), ["f0", "f2"]);
    assert_eq!(
        (
            product["total"].clone(),
            product["knownFileCount"].clone(),
            product["downloaded"].clone()
        ),
        (json!(2), json!(2), json!(1))
    );
    assert_eq!(product["manifestComplete"], true);
    assert_eq!(product["status"], "partial");
    // Any other failure in the same download is still Attention.
    failed(
        &conn,
        json!({"f1":refusal,"f2":{"fileId":"f2","errorCode":"network_failed"}}),
    );
    attempt(&conn, 2, Some("network_failed"));
    assert_eq!(projected(&conn)["status"], "attention");
    failed(&conn, json!({"f1":refusal}));
    artifact(&conn, 2, "ready", None);
    assert_eq!(projected(&conn)["status"], "ready");
    // A file downloaded before stays as a kept row, usable even while a sync asks to
    // verify the store's files: there is nothing left to verify it against.
    artifact(&conn, 1, "ready", None);
    let mut file = raw(&conn, "remote_file", "f1").unwrap().unwrap();
    file["requiresContentVerification"] = json!(true);
    set_raw(&conn, "remote_file", "f1", &file).unwrap();
    let product = projected(&conn);
    assert_eq!(listed(&product), ["f0", "f1", "f2"]);
    assert_eq!(
        (
            product["files"][1]["kept"].clone(),
            product["files"][1]["status"].clone(),
            product["keptFiles"].clone()
        ),
        (json!(true), json!("ready"), json!(1))
    );
    assert_eq!(product["status"], "ready");
    // A new attempt replaces the refusal, so the file is listed again.
    conn.execute("DELETE FROM entities WHERE kind='artifact' AND id='f1'", [])
        .unwrap();
    assert_eq!(listed(&projected(&conn)), ["f0", "f2"]);
    set_raw(&conn,"artifact","pending-f1",&json!({"id":"pending-f1","productId":"p","fileId":"f1","revision":null,"status":"downloading","originalAvailable":false})).unwrap();
    assert_eq!(listed(&projected(&conn)), ["f0", "f1", "f2"]);
}
