use super::*;
#[cfg(windows)]
#[test]
fn a_move_waits_out_a_brief_exclusive_handle_but_not_a_missing_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = std::env::temp_dir().join(format!("nna-busy-move-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let (part, published) = (root.join("authored.part"), root.join("authored.zip"));
    fs::write(&part, b"authored payload").unwrap();
    // As an antivirus scan does: an open handle that shares neither writes nor deletes.
    let scan = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&part)
        .unwrap();
    assert_eq!(
        fs::rename(&part, &published).unwrap_err().raw_os_error(),
        Some(32)
    );
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(scan);
    });
    retry_busy(|| fs::rename(&part, &published)).unwrap();
    release.join().unwrap();
    assert_eq!(fs::read(&published).unwrap(), b"authored payload");
    let started = std::time::Instant::now();
    assert!(retry_busy(|| fs::rename(root.join("missing.part"), &published)).is_err());
    assert!(started.elapsed() < std::time::Duration::from_millis(50));
    fs::remove_dir_all(&root).unwrap();
}
#[test]
fn completed_file_does_not_remove_a_waiting_transfers_shared_parent() {
    let root = std::env::temp_dir().join(format!("nna-partial-parent-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let parent = root.join(".nna-partials");
    fs::create_dir(&parent).unwrap();
    {
        let _job = EmptyDirectoryGuard(parent.clone());
        let first = parent.join("first.part");
        let second = parent.join("second.part");
        let pending = PartialGuard::new(second.clone());
        fs::write(&first, b"completed authored bytes").unwrap();
        drop(PartialGuard::new(first.clone()));
        assert!(!first.exists());
        assert!(parent.is_dir());
        fs::write(&second, b"later authored bytes").unwrap();
        drop(pending);
        assert!(!second.exists());
        assert!(!parent.exists());
    }
    assert!(!parent.exists());
    fs::remove_dir(root).unwrap();
}

#[test]
fn provider_identity_and_missing_file_errors_preserve_existing_action_codes() {
    for (error, expected) in [
        ("ACCOUNT_IDENTITY_MISMATCH", "auth_required"),
        ("STORE_IDENTITY_CHANGED", "auth_required"),
        ("FILE_UNAVAILABLE", "store_file_unavailable"),
        ("file_missing", "file_missing"),
        ("SIGNED_LINK_EXPIRED", "link_expired"),
        ("PERMISSION_DENIED", "store_access_denied"),
        ("FREE_OFFER_UNAVAILABLE", "free_offer_unavailable"),
        ("archive_corrupt", "archive_corrupt"),
        ("archive_listing_invalid", "archive_listing_invalid"),
        ("archive_inventory_mismatch", "archive_inventory_mismatch"),
        ("storage.destination_not_fresh", "destination_not_fresh"),
        ("NETWORK_UNAVAILABLE", "network_failed"),
        ("network_failed", "network_failed"),
        ("network_timeout", "network_timeout"),
        ("RATE_LIMITED", "rate_limited"),
        ("SOURCE_CHANGED", "source_changed"),
        ("download_size_mismatch", "download_size_mismatch"),
        ("DOWNLOAD_RESOLVER_FAILED", "download_resolver_failed"),
        ("DOWNLOAD_HTTP_FAILED", "download_http_failed"),
        ("download_failed", "download_http_failed"),
        ("manifest_changed", "manifest_changed"),
    ] {
        assert_eq!(stable_error(&anyhow::anyhow!(error)), expected);
    }
    assert_eq!(
        stable_error(&anyhow::anyhow!("UNRECOGNIZED_PROVIDER_FAILURE")),
        "storage_failed"
    );
}

#[cfg(windows)]
#[test]
fn locked_files_and_vanished_library_drives_keep_their_own_codes() {
    let os = |code| {
        stable_error(&anyhow::Error::from(std::io::Error::from_raw_os_error(
            code,
        )))
    };
    assert_eq!((os(32), os(33)), ("file_in_use", "file_in_use"));
    for code in [21, 53, 59, 64, 67, 1231] {
        assert_eq!(os(code), "root_offline", "{code}");
    }
    assert_eq!((os(5), os(112)), ("permission_denied", "low_space"));
}

#[test]
fn windows_shell_paths_normalize_only_supported_verbatim_prefixes() {
    assert_eq!(
        windows_shell_path(r"\\?\E:\Synthetic Library\衣装").unwrap(),
        r"E:\Synthetic Library\衣装"
    );
    assert_eq!(
        windows_shell_path(r"\\?\UNC\synthetic-server\share\衣装").unwrap(),
        r"\\synthetic-server\share\衣装"
    );
    assert_eq!(
        windows_shell_path(r"E:\Synthetic Library").unwrap(),
        r"E:\Synthetic Library"
    );
    assert_eq!(
        windows_shell_path(r"\\synthetic-server\share\衣装").unwrap(),
        r"\\synthetic-server\share\衣装"
    );
    assert!(windows_shell_path(r"\\?\GLOBALROOT\Device\HarddiskVolume1").is_err());
    assert!(windows_shell_path(r"\\.\PhysicalDrive0").is_err());
    assert!(windows_shell_path("E:\\Synthetic\0Ignored").is_err());
}

#[cfg(windows)]
#[test]
fn library_usage_sums_nested_files_without_following_junctions() {
    use std::os::windows::process::CommandExt;
    let root = std::env::temp_dir().join(format!("nna-usage-{}", uuid::Uuid::new_v4()));
    let library = root.join("library");
    fs::create_dir_all(library.join("assets/衣装/Textures")).unwrap();
    fs::write(library.join("catalogue.sqlite3"), [0; 3]).unwrap();
    fs::write(library.join("assets/衣装/Textures/body.png"), [0; 4096]).unwrap();
    fs::write(root.join("outside.bin"), [0; 1000]).unwrap();
    // A junction back to the parent would both escape the library and loop.
    let link = library.join("assets").join("loop");
    let output = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&root)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(tree_bytes(&library).unwrap(), 4099);
    fs::remove_dir(link).unwrap();
    fs::remove_dir_all(root).unwrap();
}
