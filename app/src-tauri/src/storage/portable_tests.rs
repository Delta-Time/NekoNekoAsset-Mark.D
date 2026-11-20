
use super::*;
use serde_json::json;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nna-portable-synthetic-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn path(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
    fn file(&self, path: &str, bytes: &[u8]) -> PathBuf {
        let target = self.path(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, bytes).unwrap();
        target
    }
    fn payload(&self, root: &str, relative: &str, bytes: &[u8]) -> PayloadSource {
        PayloadSource {
            root_id: root.into(),
            relative_path: relative.into(),
            source_path: self.file(&format!("source/{root}/{relative}"), bytes),
            expected_hash: Some(hex::encode(Sha256::digest(bytes))),
            is_directory: false,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = remove_staging_tree(&self.0);
    }
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn full_backup_restores_files_images_and_unchanged_catalogue_semantics() {
    let fixture = Fixture::new();
    let payload = vec![
        fixture.payload(
            "assets",
            "Base/123/revision1/衣装.unitypackage",
            b"synthetic package",
        ),
        fixture.payload("media", "custom/image.png", b"synthetic image bytes"),
        fixture.payload("assets", "Base/123/revision1/empty.txt", b""),
    ];
    let catalogue = json!({"version":1,"records":{
        "overlay":[{"id":"123","name":"Human name","rejectedTags":["wrong"],"thumbnail":"custom/image.png"}],
        "acquisition":[{"id":"unresolved-source","productId":null,"source":"gift"}],
        "entitlement":[{"id":"ent","accountId":"a","fileIds":["f"],"manifestComplete":false}],
        "root":[{"id":"assets"}],"collection":[{"id":"c","productIds":["123"]}]
    }});
    let backup = fixture.path("full.zip");
    let summary = create_backup(&backup, catalogue.clone(), &payload, &active()).unwrap();
    assert_eq!(summary.file_count, 3);
    let restored_path = fixture.path("restored");
    let restored = restore_backup(&backup, &restored_path, &active()).unwrap();
    assert_eq!(restored.catalogue, catalogue);
    assert_eq!(restored.payload.len(), 3);
    for (source, entry) in payload.iter().zip(restored.payload) {
        assert_eq!(
            fs::read(restored_path.join(entry.archive_path)).unwrap(),
            fs::read(&source.source_path).unwrap()
        );
    }
    assert!(payload[0].source_path.exists());
}

#[test]
fn empty_nested_directories_survive_backup_restore_and_migration() {
    let fixture = Fixture::new();
    let mut payload = vec![fixture.payload("root", "original/file.bin", b"good")];
    let empty = fixture.path("source/root/original/empty/nested");
    fs::create_dir_all(&empty).unwrap();
    payload.push(PayloadSource {
        root_id: "root".into(),
        relative_path: "original/empty/nested".into(),
        source_path: empty,
        expected_hash: None,
        is_directory: true,
    });
    // Explicit parent entries may come after children.
    payload.push(PayloadSource {
        root_id: "root".into(),
        relative_path: "original".into(),
        source_path: fixture.path("source/root/original"),
        expected_hash: None,
        is_directory: true,
    });
    create_backup(
        &fixture.path("dirs.zip"),
        json!({"version":1}),
        &payload,
        &active(),
    )
    .unwrap();
    let restored = restore_backup(
        &fixture.path("dirs.zip"),
        &fixture.path("restored"),
        &active(),
    )
    .unwrap();
    assert!(fixture
        .path(&format!("restored/{}", restored.payload[1].archive_path))
        .is_dir());
    let receipt = copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("moved"),
        &payload,
        &active(),
    )
    .unwrap();
    assert!(fixture.path("moved/original/empty/nested").is_dir());
    assert_eq!(cleanup_old_root(&receipt, &active()).unwrap(), 1);
    assert!(fixture.path("source/root/original/empty/nested").is_dir());
}

#[test]
fn existing_destinations_and_nonempty_restore_roots_are_preserved() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "file.bin", b"good")];
    let backup = fixture.file("existing.zip", b"keep this existing backup");
    assert!(create_backup(&backup, json!({}), &payload, &active()).is_err());
    assert_eq!(fs::read(&backup).unwrap(), b"keep this existing backup");
    create_backup(&fixture.path("new.zip"), json!({}), &payload, &active()).unwrap();
    fixture.file("occupied/unrelated.txt", b"keep");
    assert!(restore_backup(
        &fixture.path("new.zip"),
        &fixture.path("occupied"),
        &active()
    )
    .is_err());
    assert_eq!(
        fs::read(fixture.path("occupied/unrelated.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn existing_empty_picker_directories_are_valid_restore_and_migration_targets() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "file.bin", b"good")];
    create_backup(
        &fixture.path("full.zip"),
        json!({"version":1}),
        &payload,
        &active(),
    )
    .unwrap();
    fs::create_dir(fixture.path("restore-picker")).unwrap();
    let restored = restore_backup(
        &fixture.path("full.zip"),
        &fixture.path("restore-picker"),
        &active(),
    )
    .unwrap();
    assert_eq!(
        fs::read(
            fixture
                .path("restore-picker")
                .join(&restored.payload[0].archive_path)
        )
        .unwrap(),
        b"good"
    );
    fs::create_dir(fixture.path("move-picker")).unwrap();
    copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("move-picker"),
        &payload,
        &active(),
    )
    .unwrap();
    assert_eq!(
        fs::read(fixture.path("move-picker/file.bin")).unwrap(),
        b"good"
    );
    assert!(payload[0].source_path.exists());
}

#[test]
fn exclusive_file_publication_does_not_replace_a_late_destination() {
    let fixture = Fixture::new();
    let temporary = fixture.file("temporary.part", b"new bytes");
    let destination = fixture.file("destination.zip", b"existing bytes");
    assert!(publish_file_exclusive(&temporary, &destination).is_err());
    assert_eq!(fs::read(temporary).unwrap(), b"new bytes");
    assert_eq!(fs::read(destination).unwrap(), b"existing bytes");
}

#[test]
fn cancelled_operations_publish_nothing_and_preserve_sources() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "file.bin", b"good")];
    let cancelled = AtomicBool::new(true);
    assert!(create_backup(
        &fixture.path("cancelled.zip"),
        json!({}),
        &payload,
        &cancelled
    )
    .is_err());
    assert!(export_zip(&fixture.path("cancelled-export.zip"), &payload, &cancelled).is_err());
    create_backup(&fixture.path("good.zip"), json!({}), &payload, &active()).unwrap();
    assert!(restore_backup(
        &fixture.path("good.zip"),
        &fixture.path("restore"),
        &cancelled
    )
    .is_err());
    assert!(copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("move"),
        &payload,
        &cancelled
    )
    .is_err());
    assert!(!fixture.path("cancelled.zip").exists());
    assert!(!fixture.path("restore").exists());
    assert!(!fixture.path("move").exists());
    assert_eq!(fs::read(&payload[0].source_path).unwrap(), b"good");
}

#[test]
fn cancellation_during_stream_stops_at_bounded_chunk() {
    struct CancellingReader<'a> {
        flag: &'a AtomicBool,
        delivered: bool,
    }
    impl Read for CancellingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.delivered {
                panic!("read after cancellation");
            }
            self.delivered = true;
            buffer[..4].copy_from_slice(b"part");
            self.flag.store(true, Ordering::Release);
            Ok(4)
        }
    }
    let flag = active();
    let mut output = Vec::new();
    let result = transfer(
        &mut CancellingReader {
            flag: &flag,
            delivered: false,
        },
        &mut output,
        &flag,
    );
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(output, b"part");
}

#[test]
fn unsafe_paths_case_aliases_and_file_directory_collisions_are_refused() {
    for path in [
        "../out",
        "/absolute",
        "C:/file",
        "foo\\bar",
        "x//y",
        "x/./y",
        "x/../y",
        "file:ads",
        "NUL.txt",
        "COM1",
        "LPT².bin",
        "trail.",
        "trail ",
        "a\0b",
    ] {
        assert!(safe_relative(path).is_err(), "accepted {path:?}");
    }
    assert!(safe_relative("日本語/原名.psd").is_ok());
    let mut names = BTreeMap::new();
    insert_inventory_path(&mut names, "a/File.bin", false).unwrap();
    assert!(insert_inventory_path(&mut names, "A/file.BIN", false).is_err());
    assert!(insert_inventory_path(&mut names, "A/another.bin", false).is_err());
    assert!(insert_inventory_path(&mut names, "a", false).is_err());
    assert!(insert_inventory_path(&mut names, "a/File.bin/child", false).is_err());
}

fn authored_backup(
    fixture: &Fixture,
    name: &str,
    manifest: Value,
    entries: &[(&str, &[u8])],
) -> PathBuf {
    let path = fixture.path(name);
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.finish().unwrap().sync_all().unwrap();
    path
}

fn one_file_manifest() -> Value {
    json!({"format":FORMAT,"version":1,"catalogue":{"version":1},"inventory":[{
        "rootId":"r","relativePath":"file.bin","archivePath":"payload/72/file.bin",
        "sizeBytes":4,"hash":hex::encode(Sha256::digest(b"good"))
    }]})
}

#[test]
fn unsupported_version_is_refused_before_creating_restore_tree() {
    let fixture = Fixture::new();
    let backup = authored_backup(
        &fixture,
        "future.zip",
        json!({"format":FORMAT,"version":99}),
        &[],
    );
    let error = restore_backup(&backup, &fixture.path("restore"), &active()).unwrap_err();
    assert!(error.to_string().contains("version_unsupported"));
    assert!(!fixture.path("restore").exists());
}

#[test]
fn missing_extra_tampered_and_unsafe_backup_payloads_do_not_publish() {
    let fixture = Fixture::new();
    let manifest = one_file_manifest();
    let missing = authored_backup(&fixture, "missing.zip", manifest.clone(), &[]);
    let extra = authored_backup(
        &fixture,
        "extra.zip",
        manifest.clone(),
        &[("payload/72/file.bin", b"good"), ("extra.bin", b"x")],
    );
    let tampered = authored_backup(
        &fixture,
        "tampered.zip",
        manifest.clone(),
        &[("payload/72/file.bin", b"evil")],
    );
    let unsafe_zip = authored_backup(&fixture, "unsafe.zip", manifest, &[("../out.bin", b"good")]);
    for backup in [missing, extra, tampered, unsafe_zip] {
        assert!(restore_backup(&backup, &fixture.path("restore"), &active()).is_err());
        assert!(!fixture.path("restore").exists());
    }
    assert!(!fixture.path("out.bin").exists());
    assert!(!fs::read_dir(&fixture.0).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".nna-restore-")));
}

#[test]
fn symbolic_link_zip_entries_are_refused_before_writing_payloads() {
    let fixture = Fixture::new();
    let path = fixture.path("link.zip");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.add_symlink("payload/72/file.bin", "evil", options)
        .unwrap();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&one_file_manifest()).unwrap())
        .unwrap();
    zip.finish().unwrap().sync_all().unwrap();
    let error = restore_backup(&path, &fixture.path("restore"), &active()).unwrap_err();
    assert!(error.to_string().contains("link_unsupported"));
    assert!(!fixture.path("restore").exists());
}

#[test]
fn hash_mismatch_and_missing_source_abort_backup_without_partial_output() {
    let fixture = Fixture::new();
    let mut payload = vec![fixture.payload("root", "file.bin", b"good")];
    payload[0].expected_hash = Some("0".repeat(64));
    assert!(create_backup(&fixture.path("bad.zip"), json!({}), &payload, &active()).is_err());
    assert!(!fixture.path("bad.zip").exists());
    fs::remove_file(&payload[0].source_path).unwrap();
    assert!(export_zip(&fixture.path("missing.zip"), &payload, &active()).is_err());
    assert!(!fixture.path("missing.zip").exists());
}

#[test]
fn space_preflight_refuses_impossible_allocation_without_creating_files() {
    let fixture = Fixture::new();
    let error = require_space(&fixture.0, u64::MAX - 1024 * 1024).unwrap_err();
    assert!(error.to_string().contains("low_space"));
    assert!(fs::read_dir(&fixture.0).unwrap().next().is_none());
}

#[test]
fn zip_export_contains_only_requested_files_and_no_manifest() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("r", "package/file.psd", b"synthetic layered source")];
    let destination = fixture.path("selection.zip");
    export_zip(&destination, &payload, &active()).unwrap();
    let mut zip = ZipArchive::new(File::open(destination).unwrap()).unwrap();
    assert_eq!(zip.len(), 1);
    assert!(zip.by_name("manifest.json").is_err());
    let mut bytes = Vec::new();
    zip.by_name("payload/72/package/file.psd")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"synthetic layered source");
}

#[test]
fn migration_copies_only_managed_files_and_cleanup_preserves_unrelated_files() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "managed/file.bin", b"good")];
    fixture.file("source/root/user-unrelated.txt", b"leave alone");
    let receipt = copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("moved"),
        &payload,
        &active(),
    )
    .unwrap();
    let receipt: MigrationReceipt =
        serde_json::from_value(serde_json::to_value(receipt).unwrap()).unwrap();
    assert!(payload[0].source_path.exists());
    assert_eq!(
        fs::read(fixture.path("moved/managed/file.bin")).unwrap(),
        b"good"
    );
    assert!(!fixture.path("moved/user-unrelated.txt").exists());
    assert_eq!(cleanup_old_root(&receipt, &active()).unwrap(), 1);
    assert_eq!(cleanup_old_root(&receipt, &active()).unwrap(), 0);
    assert!(fixture.path("source/root").exists());
    assert_eq!(
        fs::read(fixture.path("source/root/user-unrelated.txt")).unwrap(),
        b"leave alone"
    );
}

#[test]
fn cleanup_preflights_all_files_and_rejects_changed_old_or_destination() {
    let fixture = Fixture::new();
    let payload = vec![
        fixture.payload("root", "first.bin", b"one"),
        fixture.payload("root", "second.bin", b"two"),
    ];
    let receipt = copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("moved"),
        &payload,
        &active(),
    )
    .unwrap();
    fs::write(fixture.path("moved/second.bin"), b"bad").unwrap();
    assert!(cleanup_old_root(&receipt, &active()).is_err());
    assert!(payload[0].source_path.exists());
    fs::write(fixture.path("moved/second.bin"), b"two").unwrap();
    fs::write(&payload[1].source_path, b"new").unwrap();
    assert!(cleanup_old_root(&receipt, &active()).is_err());
    assert!(payload[0].source_path.exists());
    assert_eq!(fs::read(&payload[1].source_path).unwrap(), b"new");
}

#[test]
fn migration_refuses_unmanaged_sources_overlaps_and_occupied_targets() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "file.bin", b"good")];
    assert!(copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("source/root/inside"),
        &payload,
        &active()
    )
    .is_err());
    fixture.file("occupied/unrelated", b"keep");
    assert!(copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("occupied"),
        &payload,
        &active()
    )
    .is_err());
    let mut bad = payload.clone();
    bad[0].source_path = fixture.file("other/file.bin", b"good");
    assert!(copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("moved"),
        &bad,
        &active()
    )
    .is_err());
    assert!(payload[0].source_path.exists());
    assert!(!fixture.path("moved").exists());
}

#[test]
fn cleanup_cancelled_keeps_all_old_files() {
    let fixture = Fixture::new();
    let payload = vec![fixture.payload("root", "file.bin", b"good")];
    let receipt = copy_verified_root(
        &fixture.path("source/root"),
        &fixture.path("moved"),
        &payload,
        &active(),
    )
    .unwrap();
    assert!(cleanup_old_root(&receipt, &AtomicBool::new(true)).is_err());
    assert!(payload[0].source_path.exists());
}

#[cfg(unix)]
#[test]
fn symbolic_payload_and_parent_links_are_refused() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let source = fixture.file("source/file.bin", b"good");
    symlink(&source, fixture.path("linked.bin")).unwrap();
    assert!(open_regular(&fixture.path("linked.bin")).is_err());
    symlink(fixture.path("source"), fixture.path("alias")).unwrap();
    assert!(open_regular(&fixture.path("alias/file.bin")).is_err());
    fs::remove_file(fixture.path("linked.bin")).unwrap();
    fs::remove_file(fixture.path("alias")).unwrap();
}
