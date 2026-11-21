use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
fn crash_transaction_child() {
    let Some(root) = std::env::var_os("NNA_SHARED_CRASH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let catalogue =
        Catalogue::open_bound(&root, &root.parent().unwrap().join("child-device")).unwrap();
    catalogue
        .put(
            "product",
            "authored-crash",
            json!({"id":"authored-crash","title":"Committed authored title"}),
        )
        .unwrap();
    let mut conn = catalogue.inner.lock().unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute("UPDATE entities SET body=json_set(body,'$.title','Uncommitted authored title') WHERE kind='product' AND id='authored-crash'", []).unwrap();
    tx.execute("UPDATE products SET title='Uncommitted authored title',body=json_set(body,'$.title','Uncommitted authored title'),summary=json_set(summary,'$.title','Uncommitted authored title') WHERE id='authored-crash'", []).unwrap();
    println!("SHARED_TRANSACTION_UNCOMMITTED");
    std::io::stdout().flush().unwrap();
    let mut exit = [0];
    std::io::stdin().read_exact(&mut exit).unwrap();
    drop(tx);
}

#[test]
fn process_crash_rolls_back_shared_transaction_and_releases_library() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("shared-crash-{}", Uuid::new_v4()));
    let root = fixture.join("library");
    std::fs::create_dir_all(&root).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "catalogue::shared::tests::crash_transaction_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NNA_SHARED_CRASH_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        assert!(
            output.read_line(&mut line).unwrap() != 0,
            "authored child did not reach open transaction"
        );
        if line.contains("SHARED_TRANSACTION_UNCOMMITTED") {
            break;
        }
        line.clear();
    }
    assert_eq!(
        Catalogue::open_bound(&root, &fixture.join("second-device"))
            .err()
            .unwrap()
            .to_string(),
        "library_in_use"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let catalogue = Catalogue::open_bound(&root, &fixture.join("second-device")).unwrap();
    assert_eq!(
        catalogue.get("product", "authored-crash").unwrap().unwrap()["title"],
        "Committed authored title"
    );
    assert_eq!(
        catalogue
            .command("product_details", json!({"id":"authored-crash"}))
            .unwrap()["title"],
        "Committed authored title"
    );
    let integrity: String = catalogue
        .inner
        .lock()
        .unwrap()
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    drop(catalogue);
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn shared_catalogue_retains_edits_files_and_thumbnails_at_another_mount() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("shared-catalogue-{}", Uuid::new_v4()));
    let first = fixture.join("desktop-mount");
    let second = fixture.join("laptop-mount");
    std::fs::create_dir_all(&first).unwrap();
    let a = Catalogue::open_bound(&first, &fixture.join("device-a")).unwrap();
    let root = a.list("root").unwrap().remove(0);
    let id = text(&root, "id").to_owned();
    assert_eq!(root["path"], ".");
    let mode: String = a
        .inner
        .lock()
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    assert_eq!(
        a.get("preferences", "app").unwrap().unwrap()["setupComplete"],
        false
    );
    a.put_many(vec![
            ("product".into(), "p".into(), json!({"id":"p","provider":"local","store":"Local Files","title":"Authored local","localManifest":{"complete":true,"fileIds":["f"],"directories":[]}})),
            ("remote_file".into(), "f".into(), json!({"id":"f","productId":"p","name":"Authored.txt","originKind":"local","revision":"r1"})),
            ("artifact".into(), "a".into(), json!({"id":"a","productId":"p","fileId":"f","rootId":id,"relativePath":"assets/Authored/Loose.txt","revision":"r1","status":"ready","originalAvailable":true})),
        ]).unwrap();
    std::fs::create_dir_all(first.join("assets/Authored")).unwrap();
    std::fs::write(first.join("assets/Authored/Loose.txt"), b"authored bytes").unwrap();
    std::fs::create_dir_all(a.data_dir().join("media")).unwrap();
    std::fs::write(
        a.data_dir().join("media/authored.png"),
        b"authored image bytes",
    )
    .unwrap();
    a.command("overlay", json!({"ids":["p"],"patch":{"title":"Human title","note":"Human note","thumbnailPath":"media/authored.png"}})).unwrap();
    a.command(
        "preferences",
        json!({"patch":{"setupComplete":true,"autoDownloadAll":false}}),
    )
    .unwrap();
    let clone = a.clone();
    drop(a);
    assert_eq!(
        Catalogue::open_bound(&first, &fixture.join("device-b"))
            .err()
            .unwrap()
            .to_string(),
        "library_in_use"
    );
    let device_a = clone.device_id().to_owned();
    drop(clone);
    std::fs::rename(&first, &second).unwrap();
    let b = Catalogue::open_bound(&second, &fixture.join("device-b")).unwrap();
    assert_ne!(device_a, b.device_id());
    assert_eq!(b.list("root").unwrap()[0]["id"], id);
    assert_eq!(b.list("root").unwrap()[0]["path"], ".");
    assert_eq!(
        b.command("state", json!({})).unwrap()["roots"][0]["path"],
        json!(second.canonicalize().unwrap())
    );
    let p = b.command("product_details", json!({"id":"p"})).unwrap();
    assert_eq!(p["title"], "Human title");
    assert_eq!(p["note"], "Human note");
    assert_eq!(p["status"], "ready");
    assert_eq!(p["thumbnailPath"], "media/authored.png");
    assert_eq!(
        p["previewUrl"],
        json!(b.data_dir().join("media/authored.png"))
    );
    assert_eq!(
        std::fs::read(second.join("assets/Authored/Loose.txt")).unwrap(),
        b"authored bytes"
    );
    assert_eq!(
        std::fs::read(b.data_dir().join("media/authored.png")).unwrap(),
        b"authored image bytes"
    );
    assert_eq!(
        b.get("preferences", "app").unwrap().unwrap()["setupComplete"],
        true
    );
    drop(b);
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn shared_open_rejects_newer_schema_without_upgrade_and_releases_lock() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("shared-schema-{}", Uuid::new_v4()));
    let data = fixture.join(".nna/data");
    std::fs::create_dir_all(&data).unwrap();
    let database = data.join("catalogue.sqlite3");
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version=2")
        .unwrap();
    assert_eq!(
        Catalogue::open_bound(&fixture, &fixture.join("device"))
            .err()
            .unwrap()
            .to_string(),
        "catalogue.newer_schema"
    );
    let version: i64 = Connection::open(database)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    drop(LibraryLock::acquire(&fixture).unwrap());
    std::fs::remove_dir_all(fixture).unwrap();
}

#[cfg(windows)]
#[test]
fn setup_probe_uri_accepts_unc_without_a_sqlite_uri_authority() {
    let mut uri =
        setup_probe_uri(Path::new(r"\\?\UNC\nna-probe\share\catalogue #%.sqlite3")).unwrap();
    assert_eq!(
        uri,
        "file:////nna-probe/share/catalogue%20%23%25.sqlite3?immutable=1"
    );
    // Exercise SQLite's URI parser without opening a network connection.
    uri.push_str("&mode=memory");
    Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
}
