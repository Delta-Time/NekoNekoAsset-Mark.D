//! Schema checks, the sync journal and history, restart, backup and restore.
use super::*;

#[test]
fn unsupported_schema_is_rejected_before_persistent_journal_changes() {
    let fixture = Fixture::new();
    let path = fixture.0.join("future.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=99; CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES('preserved');").unwrap();
    drop(conn);
    assert!(Catalogue::from_connection(
        Connection::open(&path).unwrap(),
        &fixture.0,
        &fixture.0,
        Some(fixture.0.clone()),
        None
    )
    .is_err());
    let conn = Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        conn.query_row("SELECT value FROM future_data", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "preserved"
    );
}

#[test]
fn shared_catalogue_checks_delete_journal_and_setup_accepts_memory_mode() {
    let fixture = Fixture::new();
    let path = fixture.0.join("shared.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    let c = Catalogue::from_connection(conn, &fixture.0, &fixture.0, Some(fixture.0.clone()), None)
        .unwrap();
    assert_eq!(
        c.inner
            .lock()
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    drop(c);
    let setup = Catalogue::open_setup(&fixture.0).unwrap();
    assert_eq!(
        setup
            .inner
            .lock()
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "memory"
    );
}

#[test]
fn restore_waits_for_live_and_queued_catalogue_work() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let live = c.begin_catalogue_work().unwrap();
    c.put(
        "job",
        "pending",
        json!({"id":"pending","type":"download","state":"cancelled"}),
    )
    .unwrap();
    assert_eq!(
        c.begin_restore_work().err().unwrap().to_string(),
        "storage_busy"
    );
    drop(live);

    c.put(
        "job",
        "pending",
        json!({"id":"pending","type":"download","state":"queued"}),
    )
    .unwrap();
    assert_eq!(
        c.begin_restore_work().err().unwrap().to_string(),
        "storage_busy"
    );
    c.put(
        "job",
        "pending",
        json!({"id":"pending","type":"download","state":"cancelled"}),
    )
    .unwrap();

    let restore = c.begin_restore_work().unwrap();
    assert_eq!(
        c.begin_catalogue_work().err().unwrap().to_string(),
        "storage_busy"
    );
    drop(restore);
    assert!(c.begin_catalogue_work().is_ok());
}

#[test]
fn current_summary_schema_reopens_without_reprojection_or_history_changes() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "1");
    let expected = dynamic_details(&c, "1");
    let history = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    let revisions = c.change_domains().unwrap();
    drop(c);
    let reopened = fixture.open();
    let result = reopened
        .command("query", json!({"includeFiles":false}))
        .unwrap();
    assert_eq!(result["products"][0]["id"], "1");
    assert_eq!(result["products"][0]["title"], expected["title"]);
    assert_eq!(result["products"][0]["filesLoaded"], false);
    assert_eq!(dynamic_details(&reopened, "1"), expected);
    assert_eq!(reopened.change_domains().unwrap(), revisions);
    assert_eq!(
        Catalogue::document_snapshot(&reopened.inner.lock().unwrap()).unwrap(),
        history
    );
}

#[test]
fn incompatible_schema_is_rejected_without_altering_records() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "a");
    c.inner.lock().unwrap().execute_batch("DROP TRIGGER products_changed_update; DROP INDEX products_avatar_evidence; ALTER TABLE products DROP COLUMN summary;").unwrap();
    drop(c);
    assert!(Catalogue::open(&fixture.0).is_err());
    let conn = Connection::open(fixture.0.join("catalogue.sqlite3")).unwrap();
    assert!(conn.prepare("SELECT summary FROM products").is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM products", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn sqlite_commands_and_bootstrap_do_not_load_sync_history() {
    let f = Fixture::new();
    let c = f.open();
    c.inner
        .lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO meta(key,value) VALUES('automerge',?1)",
            [b"synthetic invalid history".as_slice()],
        )
        .unwrap();
    let _preparation = c.document_preparation.lock().unwrap();
    product(&c, "local");
    c.command(
        "collection",
        json!({"id":"group","name":"Local collection"}),
    )
    .unwrap();
    c.command("preferences", json!({"patch":{"locale":"ja"}}))
        .unwrap();
    c.put("job","progress",json!({"id":"progress","productId":"local","type":"download","state":"running","progress":25})).unwrap();
    assert!(c
        .command("bootstrap", json!({"includeProducts":false}))
        .is_ok());
    assert!(c.command("state", json!({})).is_ok());
    assert!(c.document_cache.lock().unwrap().is_none());
    let conn = c.inner.lock().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM am_changes", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn outbox_restart_replays_order_and_ack_is_atomic_with_delta() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.command("overlay", json!({"ids":["1"],"patch":{"note":"first"}}))
        .unwrap();
    c.command("overlay", json!({"ids":["1"],"patch":{"note":"second"}}))
        .unwrap();
    drop(c);
    let c = f.open();
    c.inner.lock().unwrap().execute_batch("CREATE TRIGGER synthetic_fail_delta BEFORE INSERT ON am_changes BEGIN SELECT RAISE(FAIL,'synthetic transaction failure'); END;").unwrap();
    assert!(c.flush_sync_outbox().is_err());
    {
        let conn = c.inner.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM am_changes", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        conn.execute_batch("DROP TRIGGER synthetic_fail_delta;")
            .unwrap();
    }
    c.flush_sync_outbox().unwrap();
    assert_eq!(
        sync::entities(c.document_cache.lock().unwrap().as_ref().unwrap())
            .unwrap()
            .into_iter()
            .find(|(kind, id, _)| kind == "overlay" && id == "1")
            .unwrap()
            .2["note"],
        "second"
    );
    assert_eq!(
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let count = c
        .inner
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM am_changes", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    c.flush_sync_outbox().unwrap();
    assert_eq!(
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM am_changes", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        count
    );
    c.command("overlay", json!({"ids":["1"],"patch":{"note":"third"}}))
        .unwrap();
    c.flush_sync_outbox().unwrap();
    assert_eq!(
        c.prepare_document(
            || panic!("warm history must not wait"),
            |_, _| panic!("warm history must not decode")
        )
        .unwrap(),
        (0, 0)
    );
}

#[test]
fn long_history_is_compacted_once_without_changing_the_document() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    for n in 0..=COMPACT_HISTORY_AFTER {
        c.command(
            "overlay",
            json!({"ids":["1"],"patch":{"note":format!("note {n}")}}),
        )
        .unwrap();
        c.flush_sync_outbox().unwrap();
    }
    let deltas = |c: &Catalogue| {
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM am_changes", [], |row| {
                row.get::<_, usize>(0)
            })
            .unwrap()
    };
    let document =
        |c: &Catalogue| sync::entities(c.document_cache.lock().unwrap().as_ref().unwrap()).unwrap();
    assert!(deltas(&c) > COMPACT_HISTORY_AFTER);
    let expected = document(&c);
    drop(c);
    let c = f.open();
    c.flush_sync_outbox().unwrap();
    assert_eq!(deltas(&c), 0);
    assert_eq!(document(&c), expected);
    c.command(
        "overlay",
        json!({"ids":["1"],"patch":{"note":"after compaction"}}),
    )
    .unwrap();
    c.flush_sync_outbox().unwrap();
    let expected = document(&c);
    drop(c);
    let c = f.open();
    c.flush_sync_outbox().unwrap();
    assert_eq!(deltas(&c), 1);
    assert_eq!(document(&c), expected);
}

#[test]
fn current_booth_url_restore_preserves_observations() {
    let a = Fixture::new();
    let restored = Fixture::new();
    let ca = a.open();
    seed_current_product_url(&ca, "900002");
    let acquisition = json!({"id":"synthetic-acquisition","productId":"900002","accountId":"synthetic","sourceRecordId":"stable-row"});
    ca.put("acquisition", "synthetic-acquisition", acquisition.clone())
        .unwrap();
    let old_manifest = ca.manifest().unwrap();
    let cr = restored.open();
    cr.restore_manifest(old_manifest).unwrap();
    assert_eq!(
        cr.get("product", "900002").unwrap().unwrap()["sourceUrl"],
        "https://booth.pm/ja/items/900002"
    );
    assert_eq!(
        cr.get("acquisition", "synthetic-acquisition").unwrap(),
        Some(acquisition)
    );
}

#[test]
fn fetched_thumbnail_baseline_survives_custom_reset_sync_and_restart() {
    let f = Fixture::new();
    let c = f.open();
    let original = json!({"id":"synthetic-thumb","title":"Source title","previewUrl":"https://booth.pximg.net/synthetic-original.png","sourceUrl":"https://example.org/original"});
    c.put("product", "synthetic-thumb", original.clone())
        .unwrap();
    let fetched = "https://booth.pximg.net/synthetic-fetched.png";
    c.command("overlay", json!({"ids":["synthetic-thumb"],"patch":{"sourceUrl":"https://example.org/edited","sourceThumbnailUrl":fetched,"title":"Fetched title"}})).unwrap();
    c.command(
        "overlay",
        json!({"ids":["synthetic-thumb"],"patch":{"thumbnailPath":"media/synthetic-custom.png"}}),
    )
    .unwrap();
    let p = projection(&c, "synthetic-thumb");
    assert_eq!(p["sourcePreviewUrl"], fetched);
    assert_ne!(p["previewUrl"], fetched);
    c.command("overlay", json!({"ids":["synthetic-thumb"],"clearFields":["thumbnailPath","thumbnailHidden"],"patch":{}})).unwrap();
    assert_eq!(projection(&c, "synthetic-thumb")["previewUrl"], fetched);
    // Source refresh preserves the current fetched baseline and raw provenance.
    c.put("product", "synthetic-thumb", original.clone())
        .unwrap();
    assert_eq!(c.get("product", "synthetic-thumb").unwrap(), Some(original));
    assert_eq!(projection(&c, "synthetic-thumb")["previewUrl"], fetched);
    let saved = c.manifest().unwrap();
    let other = Fixture::new();
    let peer = other.open();
    peer.restore_manifest(saved).unwrap();
    assert_eq!(
        projection(&peer, "synthetic-thumb")["sourcePreviewUrl"],
        fetched
    );
    drop(c);
    let c = f.open();
    assert_eq!(projection(&c, "synthetic-thumb")["previewUrl"], fetched);
    c.command(
        "overlay",
        json!({"ids":["synthetic-thumb"],"patch":{"sourceThumbnailUrl":""}}),
    )
    .unwrap();
    assert_eq!(projection(&c, "synthetic-thumb")["sourcePreviewUrl"], "");
    assert!(projection(&c, "synthetic-thumb")["previewUrl"]
        .as_str()
        .unwrap_or("")
        .is_empty());
    for invalid in [
        "http://booth.pximg.net/a",
        "https://example.org/a",
        "https://booth.pximg.net/a?token=secret",
        "https://booth.pximg.net/a#fragment",
        "https://u:p@booth.pximg.net/a",
        "https://booth.pximg.net:443/a",
    ] {
        assert!(c
            .command(
                "overlay",
                json!({"ids":["synthetic-thumb"],"patch":{"sourceThumbnailUrl":invalid}})
            )
            .is_err());
        assert_eq!(projection(&c, "synthetic-thumb")["sourcePreviewUrl"], "");
    }
}

#[test]
fn current_projection_restart_preserves_overlay() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "synthetic");
    c.command("overlay",json!({"ids":["synthetic"],"patch":{"title":"Human title","category":"Tool","note":"Keep this"}})).unwrap();
    drop(c);
    let c = f.open();
    let p = projection(&c, "synthetic");
    assert_eq!(p["category"], "Tool");
    assert_eq!(p["title"], "Human title");
    assert_eq!(p["note"], "Keep this");
}

#[test]
fn source_overlay_restart_and_cjk_index() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "100");
    let result=c.command("overlay",json!({"ids":["100"],"patch":{"title":"Human name","note":"備考の短文","tags":["hair"]}})).unwrap();
    c.put("product","100",json!({"id":"100","title":"New source","tags":["blue","hair","green"],"sourceCategory":{"name":"Hair"}})).unwrap();
    let p = projection(&c, "100");
    assert_eq!(p["title"], "Human name");
    assert_eq!(p["sourceTitle"], "New source");
    assert_eq!(p["tags"], json!(["hair"]));
    assert!(strings(&p["rejectedTags"]).contains(&"blue".into()));
    assert_eq!(
        c.command("query", json!({"search":"備考"})).unwrap()["matchedIds"],
        json!(["100"])
    );
    drop(c);
    let c = f.open();
    assert_eq!(projection(&c, "100")["title"], "Human name");
    c.command("overlay", json!({"ids":["100"],"patch":{"title":"Later"}}))
        .unwrap();
    assert_eq!(
        c.command("undo", json!({"token":result["token"]})).unwrap()["conflicts"],
        json!(["100"])
    );
}

#[test]
fn restore_rejects_before_replacement_and_preserves_semantics() {
    let a = Fixture::new();
    let c = a.open();
    product(&c, "1");
    c.command(
        "overlay",
        json!({"ids":["1"],"patch":{"note":"Retained","rejectedTags":["blue"]}}),
    )
    .unwrap();
    c.put("acquisition","unresolved",json!({"id":"unresolved","sourceRecordId":"observation-unknown","productId":null,"source":"purchase"})).unwrap();
    c.put(
        "root",
        "r",
        json!({"id":"r","path":"C:/synthetic/local-only","status":"online"}),
    )
    .unwrap();
    let manifest = c.manifest().unwrap();
    assert!(manifest.to_string().find("C:/synthetic").is_none());
    let mut invalid = manifest.clone();
    invalid["version"] = json!(999);
    assert!(c.restore_manifest(invalid).is_err());
    assert_eq!(projection(&c, "1")["note"], "Retained");
    let b = Fixture::new();
    let other = b.open();
    other.restore_manifest(manifest).unwrap();
    assert_eq!(projection(&other, "1")["note"], "Retained");
    assert_eq!(
        other.get("root", "r").unwrap().unwrap()["status"],
        "needsMapping"
    );
    assert!(other.get("acquisition", "unresolved").unwrap().is_some());
}

#[test]
fn restore_then_mutation_restart_does_not_resurrect() {
    let a = Fixture::new();
    let b = Fixture::new();
    let ca = a.open();
    let cb = b.open();
    product(&ca, "old");
    product(&cb, "new");
    let manifest = cb.manifest().unwrap();
    ca.restore_manifest(manifest).unwrap();
    ca.command(
        "overlay",
        json!({"ids":["new"],"patch":{"note":"after restore"}}),
    )
    .unwrap();
    drop(ca);
    let ca = a.open();
    assert!(ca.get("product", "old").unwrap().is_none());
    assert_eq!(projection(&ca, "new")["note"], "after restore");
    drop(ca);
    let ca = a.open();
    assert!(ca.get("product", "old").unwrap().is_none());
}

#[test]
fn lite_query_keeps_the_summary_status() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    let lite = c.command("query", json!({"includeFiles":false})).unwrap();
    assert_eq!(lite["products"][0]["filesLoaded"], false);
    assert_eq!(
        lite["products"][0]["status"],
        c.command("product_details", json!({"id":"1"})).unwrap()["status"]
    );
}

#[test]
fn conditional_batch_preserves_intervening_writes_and_negative_reads() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    let initial = c.get("product", "1").unwrap();
    let newer = json!({"id":"1","title":"Live source refresh"});
    c.put("product", "1", newer.clone()).unwrap();
    let revision = c.revision().unwrap();
    let attempted = vec![(
        "product".into(),
        "1".into(),
        json!({"id":"1","title":"Stale import"}),
    )];
    assert!(!c
        .put_many_if_unchanged(
            attempted.clone(),
            vec![("product".into(), "1".into(), initial)]
        )
        .unwrap());
    assert_eq!(c.revision().unwrap(), revision);
    assert_eq!(c.get("product", "1").unwrap(), Some(newer.clone()));
    product(&c, "2");
    let revision = c.revision().unwrap();
    let chunks: i64 = c
        .inner
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM am_changes", [], |r| r.get(0))
        .unwrap();
    assert!(!c
        .put_many_if_unchanged(
            attempted.clone(),
            vec![
                ("product".into(), "1".into(), Some(newer.clone())),
                ("product".into(), "2".into(), None)
            ]
        )
        .unwrap());
    assert_eq!(c.revision().unwrap(), revision);
    assert_eq!(
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM am_changes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        chunks
    );
    assert_eq!(c.get("product", "1").unwrap(), Some(newer.clone()));
    assert!(c
        .put_many_if_unchanged(attempted, vec![("product".into(), "1".into(), Some(newer))])
        .unwrap());
    assert_eq!(
        c.get("product", "1").unwrap().unwrap()["title"],
        "Stale import"
    );
}

#[test]
fn restart_rejects_corrupt_truncated_or_missing_middle_history() {
    for mode in ["corrupt", "truncate", "missing"] {
        let f = Fixture::new();
        let c = f.open();
        product(&c, "1");
        c.flush_sync_outbox().unwrap();
        c.command("overlay", json!({"ids":["1"],"patch":{"note":"Middle"}}))
            .unwrap();
        c.flush_sync_outbox().unwrap();
        c.command(
            "overlay",
            json!({"ids":["1"],"patch":{"title":"Latest projection"}}),
        )
        .unwrap();
        c.flush_sync_outbox().unwrap();
        {
            let conn = c.inner.lock().unwrap();
            let (seq, body): (i64, Vec<u8>) = conn
                .query_row(
                    "SELECT seq,body FROM am_changes ORDER BY seq LIMIT 1 OFFSET 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            if mode == "missing" {
                conn.execute("DELETE FROM am_changes WHERE seq=?1", [seq])
                    .unwrap();
            } else {
                let corrupt = if mode == "truncate" {
                    body[..body.len() / 2].to_vec()
                } else {
                    b"invalid middle history".to_vec()
                };
                conn.execute(
                    "UPDATE am_changes SET body=?1 WHERE seq=?2",
                    params![corrupt, seq],
                )
                .unwrap();
            }
        }
        drop(c);
        let reopened = f.open();
        assert!(reopened.ensure_document().is_err(), "{mode}");
        // Broken sync history cannot destroy or block independent local edits.
        let revision = reopened.revision().unwrap();
        reopened
            .command(
                "overlay",
                json!({"ids":["1"],"patch":{"note":"Durable local edit"}}),
            )
            .unwrap();
        assert!(reopened.revision().unwrap() > revision);
        let local = reopened.get("overlay", "1").unwrap().unwrap();
        assert_eq!(local["title"], "Latest projection");
        assert_eq!(local["note"], "Durable local edit");
        assert!(reopened.prepare_sync_document().is_err());
        assert_eq!(
            reopened
                .inner
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn concurrent_cold_preparation_loads_once_without_blocking_reads() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;

    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.flush_sync_outbox().unwrap();
    drop(c);
    let c = f.open();
    let before = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    let loads = Arc::new(AtomicUsize::new(0));
    let (validated_tx, validated_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let owner = c.clone();
    let owner_loads = loads.clone();
    let owner = std::thread::spawn(move || {
        owner.prepare_document(
            || {},
            |bytes, actor| {
                let doc = sync::load_doc(bytes, actor)?;
                owner_loads.fetch_add(1, Ordering::SeqCst);
                validated_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                Ok(doc)
            },
        )
    });
    validated_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    let (waiting_tx, waiting_rx) = mpsc::channel();
    let mut waiters = vec![];
    for _ in 0..4 {
        let waiter = c.clone();
        let waiting_tx = waiting_tx.clone();
        let loads = loads.clone();
        waiters.push(std::thread::spawn(move || {
            waiter.prepare_document(
                || waiting_tx.send(()).unwrap(),
                |bytes, actor| {
                    let doc = sync::load_doc(bytes, actor)?;
                    loads.fetch_add(1, Ordering::SeqCst);
                    Ok(doc)
                },
            )
        }));
    }
    // Every competitor has observed the cold cache before the owner installs.
    let all_waiting = (0..4).all(|_| waiting_rx.recv_timeout(Duration::from_secs(5)).is_ok());
    let reader = c.clone();
    let (read_tx, read_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        read_tx.send(reader.get("product", "1")).unwrap();
    });
    let read = read_rx.recv_timeout(Duration::from_secs(5));
    // Release workers before assertions, including on a lock regression.
    resume_tx.send(()).unwrap();
    assert!(all_waiting, "cold waiters must not hold SQLite/cache locks");
    assert_eq!(read.unwrap().unwrap().unwrap()["id"], "1");
    owner.join().unwrap().unwrap();
    for waiter in waiters {
        waiter.join().unwrap().unwrap();
    }
    reader.join().unwrap();
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(
        Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap(),
        before,
        "preparation must not alter durable history or revisions"
    );

    // A warm caller must complete even while the preparation gate is occupied.
    let gate = c.document_preparation.lock().unwrap();
    let warm = c.clone();
    let (warm_tx, warm_rx) = mpsc::channel();
    let warm = std::thread::spawn(move || {
        warm_tx
            .send(warm.prepare_document(
                || panic!("warm cache must bypass preparation"),
                |_, _| panic!("warm cache must not reload history"),
            ))
            .unwrap();
    });
    let result = warm_rx.recv_timeout(Duration::from_secs(5));
    drop(gate);
    warm.join().unwrap();
    assert_eq!(result.unwrap().unwrap(), (0, 0));
}

#[test]
fn cold_restart_small_write_persists_only_new_delta() {
    let f = Fixture::new();
    let c = f.open();
    c.put_many(
        (0..32)
            .map(|i| {
                let id = format!("history-{i}");
                (
                    "product".into(),
                    id.clone(),
                    json!({"id":id,"title":format!("Synthetic retained product {i}"),"sourceTags":["Synthetic"]}),
                )
            })
            .collect(),
    )
    .unwrap();
    c.flush_sync_outbox().unwrap();
    drop(c);

    let c = f.open();
    let history = |c: &Catalogue| -> Vec<(i64, Vec<u8>)> {
        let conn = c.inner.lock().unwrap();
        let mut statement = conn
            .prepare("SELECT seq,body FROM am_changes ORDER BY seq")
            .unwrap();
        let chunks = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        chunks
    };
    let before_chunks = history(&c);
    let before = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    c.ensure_document().unwrap();
    assert_eq!(
        Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap(),
        before
    );
    let before_heads = c
        .document_cache
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .get_heads();
    c.command(
        "collection",
        json!({"operation":"create","id":"cold-edit","value":{"name":"Synthetic cold edit","productIds":["history-0"]}}),
    )
    .unwrap();
    c.flush_sync_outbox().unwrap();
    let after_chunks = history(&c);
    assert_eq!(after_chunks.len(), before_chunks.len() + 1);
    assert_eq!(
        &after_chunks[..before_chunks.len()],
        before_chunks.as_slice()
    );
    let delta = &after_chunks.last().unwrap().1;
    assert!(
        delta.len() < before.1.len(),
        "a small edit must not append the full baseline history"
    );
    // Exactly one new change depends on the prior heads. A full-history append
    // would contain extra changes and cannot decode as this single change.
    let change = automerge::Change::from_bytes(delta.clone()).unwrap();
    assert_eq!(change.deps(), before_heads.as_slice());
    assert!(!before_heads.contains(&change.hash()));
    drop(c);

    let c = f.open();
    c.ensure_document().unwrap();
    let expected = sync::entities(c.document_cache.lock().unwrap().as_ref().unwrap()).unwrap();
    let collection = expected
        .iter()
        .find(|(kind, id, _)| kind == "collection" && id == "cold-edit")
        .unwrap();
    assert_eq!(collection.2["name"], "Synthetic cold edit");
    assert_eq!(collection.2["productIds"], json!(["history-0"]));
    assert_eq!(
        expected
            .iter()
            .filter(|(kind, _, _)| kind == "product")
            .count(),
        32
    );
}

#[test]
fn delayed_cold_document_cannot_replace_an_intervening_writer() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.flush_sync_outbox().unwrap();
    let (version, bytes, _) = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    let mut delayed = sync::load_doc(&bytes, &version.0).unwrap();
    let _ = delayed.save_incremental();
    c.command(
        "overlay",
        json!({"ids":["1"],"patch":{"note":"Newer writer"}}),
    )
    .unwrap();
    c.flush_sync_outbox().unwrap();
    // Simulate the rollback/invalidation interleaving while an older decoder
    // finishes. A changed durable generation must reject its installation.
    *c.document_cache.lock().unwrap() = None;
    assert!(!c.install_document(&version, delayed, false).unwrap());
    c.ensure_document().unwrap();
    let saved = c.manifest().unwrap();
    let restored = Fixture::new();
    let other = restored.open();
    other.restore_manifest(saved).unwrap();
    assert_eq!(projection(&other, "1")["note"], "Newer writer");
}
