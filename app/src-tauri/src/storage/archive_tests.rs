use super::*;

#[test]
fn abandoned_workspace_recovery_requires_marker_identity_and_released_lock() {
    let root = std::env::temp_dir().join(format!("nna-archive-recovery-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let active_id = uuid::Uuid::new_v4();
    let active = root.join(format!("archive-{active_id}"));
    fs::create_dir(&active).unwrap();
    let marker = active.join(WORKSPACE_MARKER);
    let mut owner = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&marker)
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&owner).unwrap();
    owner
        .write_all(format!("{WORKSPACE_OWNER}{active_id}\n").as_bytes())
        .unwrap();
    owner.sync_all().unwrap();
    fs::create_dir(active.join("content")).unwrap();
    fs::write(
        active.join("content/authored.bin"),
        b"authored uncommitted bytes",
    )
    .unwrap();
    let legacy = root.join(format!("archive-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("human.bin"), b"unmarked bytes").unwrap();
    let invalid = root.join(format!("archive-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&invalid).unwrap();
    fs::write(
        invalid.join(WORKSPACE_MARKER),
        format!("{WORKSPACE_OWNER}{}\n", uuid::Uuid::new_v4()),
    )
    .unwrap();
    assert_eq!(recover_workspaces(&root).unwrap(), 0);
    assert!(active.join("content/authored.bin").exists());
    drop(owner);
    assert_eq!(recover_workspaces(&root).unwrap(), 1);
    assert!(!active.exists());
    assert!(legacy.join("human.bin").exists());
    assert!(invalid.join(WORKSPACE_MARKER).exists());
    fs::remove_file(legacy.join("human.bin")).unwrap();
    fs::remove_dir(legacy).unwrap();
    fs::remove_file(invalid.join(WORKSPACE_MARKER)).unwrap();
    fs::remove_dir(invalid).unwrap();
    fs::remove_dir(root).unwrap();
}

#[cfg(windows)]
#[test]
fn locked_workspace_payload_retains_ownership_proof_for_the_next_recovery() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = std::env::temp_dir().join(format!("nna-archive-locked-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    let workspace = root.join(format!("archive-{id}"));
    fs::create_dir(&workspace).unwrap();
    fs::write(
        workspace.join(WORKSPACE_MARKER),
        format!("{WORKSPACE_OWNER}{id}\n"),
    )
    .unwrap();
    let payload = workspace.join("authored.bin");
    fs::write(&payload, b"authored locked bytes").unwrap();
    let handle = File::options()
        .read(true)
        .share_mode(3)
        .open(&payload)
        .unwrap();
    assert_eq!(recover_workspaces(&root).unwrap(), 0);
    assert!(workspace.join(WORKSPACE_MARKER).exists());
    drop(handle);
    assert_eq!(recover_workspaces(&root).unwrap(), 1);
    fs::remove_dir(root).unwrap();
}

#[test]
fn portable_paths_reject_escape_and_windows_aliases() {
    for path in [
        "../x",
        "a/../../x",
        "C:\\x",
        "\\\\host\\share",
        "/x",
        "a//b",
        "a/./b",
        "a ",
        "a.",
        "CON.txt",
        "lpt¹.bin",
        "x:NNA",
        "a\nPath = evil",
        "a/aux/file",
        "COM1 .txt",
    ] {
        assert!(validate_relative_path(path).is_err(), "{path}");
    }
    assert_eq!(
        portable_path(&validate_relative_path("衣装\\素材/texture.png").unwrap()).unwrap(),
        "衣装/素材/texture.png"
    );
}

#[test]
fn written_names_follow_the_bundled_7zip() {
    // Each pair was checked against what 7-Zip 26.04 writes on Windows 11.
    for (name, written) in [
        ("a:b.png", "a_b.png"),
        ("quote\"<|>*?.txt", "quote______.txt"),
        ("名前:日本語?.png", "名前_日本語_.png"),
        ("tab\there", "tab_here"),
        ("trail..", "trail__"),
        ("trail .", "trail__"),
        ("...", "___"),
        ("x .txt", "x .txt"),
        (" lead.png", " lead.png"),
        ("\u{3000}end\u{3000}", "\u{3000}end\u{3000}"),
        ("con", "_con"),
        ("CON.txt", "_CON.txt"),
        ("con .txt", "_con .txt"),
        ("Con.tar.gz", "_Con.tar.gz"),
        ("aux. txt", "_aux. txt"),
        ("nul.", "nul_"),
        ("COM1", "_COM1"),
        ("com9.prefab", "_com9.prefab"),
        ("COM0.txt", "_COM0.txt"),
        ("LPT0", "_LPT0"),
        ("COM10.txt", "COM10.txt"),
        ("COM", "COM"),
        ("AUX_x.png", "AUX_x.png"),
        ("CONIN$.txt", "CONIN$.txt"),
        ("LPT¹.txt", "LPT¹.txt"),
    ] {
        assert_eq!(written_name(name), written, "{name:?}");
    }
}

#[test]
fn member_paths_stay_inside_and_land_where_7zip_writes_them() {
    for path in [
        "",
        "/x",
        "\\x",
        "C:x",
        "c:/x",
        "a//b",
        "a/./b",
        "a/../b",
        "..",
        "a\nPath = evil",
        "a\u{7f}b",
    ] {
        assert!(member_path(path).is_err(), "{path:?}");
    }
    assert_eq!(member_path("Ver1.0.\\a:b.png").unwrap(), "Ver1.0./a:b.png");
    assert_eq!(
        written_path("Ver1.0./Materials /aux.png").unwrap(),
        "Ver1.0_/Materials_/_aux.png"
    );
    // 7-Zip leaves these alone, and Windows 10 still reads them as devices.
    for path in ["LPT¹.txt", "x/CONIN$.txt", "CLOCK$"] {
        assert!(written_path(path).is_err(), "{path}");
    }
    let entries = parse_listing(
        &listing(
            "Path = Mac\nFolder = +\nSize = 0\n\n\
             Path = Mac/a:b.png\nSize = 1\nCRC = 1\n\n\
             Path = Mac/a_b.png\nSize = 1\nCRC = 2\n\n\
             Path = dir.\nFolder = +\nSize = 0\n\n\
             Path = dir./x.txt\nSize = 1\nCRC = 3\n\n\
             Path = dir_/y.txt\nSize = 1\nCRC = 4",
        ),
        false,
    )
    .unwrap();
    // A listing keeps the archive's own names, which select members for 7-Zip.
    assert_eq!(entries[1].relative_path, "Mac/a:b.png");
    let plan = plan_extraction(&entries).unwrap();
    let paths: Vec<_> = plan
        .outputs
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "Mac",
            "dir_",
            "Mac/a_b.png",
            "Mac/a_b_1.png",
            "dir_/x.txt",
            "dir_/y.txt"
        ]
    );
    validate_inventory_paths(&plan.outputs).unwrap();
    let unsafe_member = parse_listing(&listing("Path = LPT¹.txt\nSize = 1\nCRC = 1"), false);
    assert!(plan_extraction(&unsafe_member.unwrap()).is_err());
}

fn listing(records: &str) -> String {
    format!("7-Zip\n\nPath = source.zip\nType = zip\n\n----------\n{records}\n")
}

// Independently authored, uncompressed fixture from the public RAR5 field
// specification: https://www.rarlab.com/technote.htm . No codec source used.
fn tiny_rar_fixture(name: &str, part: Option<u8>) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        !bytes.iter().fold(!0u32, |value, byte| {
            (value >> 8) ^ CRC_TABLE[((value ^ u32::from(*byte)) & 0xff) as usize]
        })
    }
    fn block(output: &mut Vec<u8>, body: &[u8]) {
        assert!(body.len() < 128);
        let mut header = vec![body.len() as u8];
        header.extend_from_slice(body);
        output.extend_from_slice(&crc(&header).to_le_bytes());
        output.extend(header);
    }
    assert!(name.len() < 100);
    let mut output = b"Rar!\x1a\x07\x01\0".to_vec();
    let payload = match part {
        Some(0) => &b"a"[..],
        Some(1) => &b"bc"[..],
        None => &b"abc"[..],
        _ => panic!("invalid fixture part"),
    };
    match part {
        None => block(&mut output, &[1, 0, 0]),
        Some(0) => block(&mut output, &[1, 0, 1]),
        Some(_) => block(&mut output, &[1, 0, 3, 1]),
    }
    // file header, data present, packed size, CRC present, unpacked size, attributes
    let flags = match part {
        None => 2,
        Some(0) => 18,
        Some(_) => 10,
    };
    let mut file = vec![2, flags, payload.len() as u8, 4, 3, 32];
    file.extend_from_slice(&crc(if part == Some(0) { payload } else { b"abc" }).to_le_bytes());
    file.extend_from_slice(&[0, 0, name.len() as u8]); // store method, Windows, name length
    file.extend_from_slice(name.as_bytes());
    block(&mut output, &file);
    output.extend_from_slice(payload);
    block(&mut output, &[5, 0, u8::from(part == Some(0))]);
    output
}

#[test]
fn listing_accepts_warning_counts_but_never_error_or_malformed_summaries() {
    let members = "Path = normal.txt\nSize = 3\nCRC = 352441C2";
    let normal = parse_listing(&listing(members), false).unwrap();
    assert_eq!(
        parse_listing(&listing(&format!("{members}\n\n\nWarnings: 1")), false).unwrap(),
        normal
    );
    for summary in ["Errors: 1", "Warnings: unknown", "Warnings: 1\nErrors: 1"] {
        assert!(parse_listing(&listing(&format!("{members}\n\n{summary}")), false).is_err());
    }
}

#[test]
fn listing_rejects_links_and_ambiguous_records() {
    for records in [
        "Path = a\nSize = 0\nSymbolic Link = ../outside",
        "Path = a\nSize = 0\nHard Link = b",
        "Path = a\nSize = 0\nAttributes = A lrwxrwxrwx",
        "Path = a\nSize = 0\nEncrypted = +",
        "Path = a\nSize = 0\nPath = b",
    ] {
        assert!(
            parse_listing(&listing(records), false).is_err(),
            "{records}"
        );
    }
    // An encrypted member is fine once its password is given.
    assert!(parse_listing(&listing("Path = a\nSize = 0\nEncrypted = +"), true).is_ok());
    let entries = parse_listing(&listing(
        "Path = 衣装\nFolder = +\nSize = 0\n\nPath = 衣装/a.unitypackage\nSize = 3\nCRC = 352441C2",
    ), false)
    .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(!is_supported_archive(Path::new("a.unitypackage")));
}

#[test]
fn plan_merges_folder_case_and_keeps_every_clashing_file() {
    let entries = parse_listing(
        &listing(
            "Path = Dir/a.txt\nSize = 1\nCRC = 1\n\n\
             Path = dir/b.txt\nSize = 1\nCRC = 2\n\n\
             Path = DIR\nFolder = +\nSize = 0\n\n\
             Path = b.txt\nSize = 3\nCRC = A\n\n\
             Path = b.txt\nSize = 3\nCRC = A\n\n\
             Path = B.txt\nSize = 3\nCRC = B\n\n\
             Path = b_1.txt\nSize = 4\nCRC = C\n\n\
             Path = dir\nSize = 5\nCRC = D\n\n\
             Path = .hidden\nSize = 1\nCRC = E\n\n\
             Path = .hidden\nSize = 1\nCRC = F\n\n\
             Path = x.tar.gz\nSize = 1\nCRC = 1\n\n\
             Path = x.tar.gz\nSize = 2\nCRC = 1",
        ),
        false,
    )
    .unwrap();
    let plan = plan_extraction(&entries).unwrap();
    let paths: Vec<_> = plan
        .outputs
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect();
    // Folders merge into their first spelling and keep their names; a taken file
    // name gets 7-Zip's `_N` (its search sees `b_1.txt` taken, so `B.txt` is `_2`).
    assert_eq!(
        paths,
        [
            "Dir",
            "Dir/a.txt",
            "Dir/b.txt",
            "b.txt",
            "b_1.txt",
            "B_2.txt",
            "b_1_1.txt",
            "dir_1",
            ".hidden",
            ".hidden_1",
            "x.tar.gz",
            "x.tar_1.gz"
        ]
    );
    // Only the same size and CRC is an identical copy, dropped after extraction.
    assert_eq!(plan.identical, [("b_1.txt".to_owned(), "b.txt".to_owned())]);
    validate_inventory_paths(&plan.outputs).unwrap();
}

#[test]
fn volume_groups_are_contiguous_and_keep_original_paths() {
    let paths = vec![
        PathBuf::from("a/demo.7z.001"),
        PathBuf::from("b/demo.7z.002"),
        PathBuf::from("c/demo.7z.003"),
    ];
    assert_eq!(validate_volume_group(&paths[1], &paths).unwrap(), paths);
    assert!(validate_volume_group(&paths[0], &[paths[2].clone()]).is_err());
    assert!(validate_volume_group(&paths[1], &[]).is_err());
    assert!(validate_volume_group(&paths[0], &[PathBuf::from("other/demo.7z.001")]).is_err());
    let zip = vec![
        PathBuf::from("demo.zip"),
        PathBuf::from("demo.z01"),
        PathBuf::from("demo.z02"),
    ];
    assert_eq!(validate_volume_group(&zip[2], &zip).unwrap(), zip);
    let rar = vec![
        PathBuf::from("demo.part01.rar"),
        PathBuf::from("demo.part02.rar"),
    ];
    assert_eq!(validate_volume_group(&rar[1], &rar).unwrap(), rar);
}

#[test]
fn process_errors_are_fixed_actionable_codes() {
    assert_eq!(
        classify_process_error(Some(2), b"private.zip", b"Wrong password?"),
        "archive_password_required"
    );
    assert_eq!(
        classify_process_error(Some(2), b"", b"Missing volume : private.part2.rar"),
        "archive_missing_volume"
    );
    assert_eq!(
        classify_process_error(Some(2), b"", b"Unexpected end of archive"),
        "archive_corrupt"
    );
    assert_eq!(
        classify_process_error(Some(2), b"Type = Split", b"Unexpected end of archive"),
        "archive_missing_volume"
    );
    assert_eq!(
        classify_process_error(Some(2), b"", b"There is not enough space on the disk"),
        "storage_low_space"
    );
}

#[test]
fn inventory_checks_missing_extra_size_crc_and_cancel() {
    let root = std::env::temp_dir().join(format!("nna-archive-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("衣装")).unwrap();
    fs::write(root.join("衣装/a.unitypackage"), b"abc").unwrap();
    let mut entries = parse_listing(
        &listing("Path = 衣装/a.unitypackage\nSize = 3\nCRC = 352441C2"),
        false,
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);
    let result = verify_extracted_inventory(&root, &entries, &cancelled).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result[0].is_directory);
    assert_eq!(
        result[1].hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    entries[0].size_bytes = 4;
    assert!(verify_extracted_inventory(&root, &entries, &cancelled).is_err());
    entries[0].size_bytes = 3;
    entries[0].crc32 = Some(0);
    assert!(verify_extracted_inventory(&root, &entries, &cancelled).is_err());
    entries[0].crc32 = None;
    fs::write(root.join("extra"), b"").unwrap();
    assert!(verify_extracted_inventory(&root, &entries, &cancelled).is_err());
    fs::remove_file(root.join("extra")).unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(verify_extracted_inventory(&root, &entries, &cancelled).is_err());
    cancelled.store(false, Ordering::Relaxed);
    fs::remove_file(root.join("衣装/a.unitypackage")).unwrap();
    assert!(verify_extracted_inventory(&root, &entries, &cancelled).is_err());
    // This exact UUID directory was created by this test and contains synthetic bytes only.
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn inventory_rejects_hard_links() {
    let root = std::env::temp_dir().join(format!("nna-archive-link-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("first"), b"abc").unwrap();
    fs::hard_link(root.join("first"), root.join("second")).unwrap();
    let entries = parse_listing(
        &listing("Path = first\nSize = 3\n\nPath = second\nSize = 3"),
        false,
    )
    .unwrap();
    assert!(verify_extracted_inventory(&root, &entries, &AtomicBool::new(false)).is_err());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn zip_cli_uses_japanese_legacy_names_for_listing_and_extraction_only() {
    for operation in ["l", "x"] {
        for archive in ["source.zip", "source.ZIP", "source.zip.001"] {
            for code_page in [932, 65001] {
                assert!(cli_args(operation, Path::new(archive), code_page, false)
                    .contains(&OsString::from(format!("-mcp={code_page}"))));
            }
        }
        for archive in ["source.7z", "source.rar", "source.7z.001"] {
            assert!(!cli_args(operation, Path::new(archive), 932, false)
                .iter()
                .any(|arg| arg.to_string_lossy().starts_with("-mcp")));
        }
    }
}

/// Stored members of an independently authored ZIP; `utf8` sets the UTF-8 name flag.
#[cfg(windows)]
fn authored_zip(members: &[(&[u8], bool, &[u8])]) -> Vec<u8> {
    let crc = |bytes: &[u8]| {
        !bytes.iter().fold(!0u32, |value, byte| {
            (value >> 8) ^ CRC_TABLE[((value ^ u32::from(*byte)) & 0xff) as usize]
        })
    };
    let mut zip = Vec::new();
    let mut central = Vec::new();
    for (name, utf8, data) in members {
        let offset = zip.len() as u32;
        let flags: u16 = if *utf8 { 0x800 } else { 0 };
        let (crc, size) = (crc(data), data.len() as u32);
        zip.extend_from_slice(&0x04034b50u32.to_le_bytes());
        for value in [20, flags, 0, 0, 0] {
            zip.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            zip.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0] {
            zip.extend_from_slice(&value.to_le_bytes());
        }
        zip.extend_from_slice(name);
        zip.extend_from_slice(data);
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        for value in [20, 20, flags, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(&0x06054b50u32.to_le_bytes());
    for value in [0, 0, members.len() as u16, members.len() as u16] {
        zip.extend_from_slice(&value.to_le_bytes());
    }
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&central_offset.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip
}

/// Extracts with the bundled 7-Zip and returns (path, bytes) for every file.
#[cfg(windows)]
fn bundled_extract(name: &str, zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tools/7zip/7z.exe");
    let root = std::env::temp_dir().join(format!("nna-{name}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("staging")).unwrap();
    let archive = root.join("authored.zip");
    fs::write(&archive, zip).unwrap();
    let extracted = extract_archive(
        &executable,
        &archive,
        &[],
        &root.join("staging"),
        &AtomicBool::new(false),
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    let files = extracted
        .inventory
        .iter()
        .filter(|entry| !entry.is_directory)
        .map(|entry| {
            let bytes = fs::read(extracted.staging_path.join(&entry.relative_path)).unwrap();
            (entry.relative_path.clone(), bytes)
        })
        .collect();
    assert_eq!(fs::read(&archive).unwrap(), zip);
    drop(extracted);
    fs::remove_dir_all(root).unwrap();
    files
}

#[cfg(windows)]
#[test]
fn bundled_zip_reads_unmarked_utf8_names_as_utf8() {
    // Read as Japanese Windows names, these bytes held C1 controls and were refused.
    let files = bundled_extract(
        "authored-zip-utf8",
        &authored_zip(&[
            ("「テスト」.txt".as_bytes(), false, b"bracketed"),
            ("日本語フォルダ/ファイル.txt".as_bytes(), false, b"nested"),
            (b"Readme.txt", false, b"ascii"),
        ]),
    );
    let expected = [
        ("Readme.txt", &b"ascii"[..]),
        ("「テスト」.txt", b"bracketed"),
        ("日本語フォルダ/ファイル.txt", b"nested"),
    ];
    assert_eq!(files.len(), expected.len());
    for (path, bytes) in expected {
        assert!(files.contains(&(path.to_owned(), bytes.to_vec())), "{path}");
    }
}

#[cfg(windows)]
#[test]
fn bundled_zip_merges_folder_case_and_keeps_clashing_members() {
    let files = bundled_extract(
        "authored-zip-clashes",
        &authored_zip(&[
            (b"Dir/a.txt", false, b"a"),
            (b"dir/b.txt", false, b"b"),
            (b"DIR/sub/c.txt", false, b"c"),
            (b"same.txt", false, b"identical"),
            (b"same.txt", false, b"identical"),
            (b"b.txt", false, b"one"),
            (b"b.txt", false, b"two"),
            (b"B.txt", false, b"three"),
            (b"x", false, b"file"),
            (b"X/inner.txt", false, b"inner"),
        ]),
    );
    // What 7-Zip actually wrote matches the plan; one identical copy remains.
    let expected: Vec<_> = [
        ("B_2.txt", &b"three"[..]),
        ("Dir/a.txt", b"a"),
        ("Dir/b.txt", b"b"),
        ("Dir/sub/c.txt", b"c"),
        ("X/inner.txt", b"inner"),
        ("b.txt", b"one"),
        ("b_1.txt", b"two"),
        ("same.txt", b"identical"),
        ("x_1", b"file"),
    ]
    .into_iter()
    .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
    .collect();
    assert_eq!(files, expected);
}

#[cfg(windows)]
#[test]
fn bundled_zip_writes_names_windows_cannot_store_as_7zip_does() {
    // Names a macOS or Linux packer can write. Each lands under 7-Zip's own name for it,
    // a clash with a name already there still gets `_N`, and the rest extract as usual.
    let files = bundled_extract(
        "authored-zip-unsafe-names",
        &authored_zip(&[
            (b"Mac/a:b?.png", false, b"colon"),
            (b"Mac/a_b_.png", false, b"taken"),
            (b"Mac/quote\"<|>*.txt", false, b"marks"),
            (b"Ver1.0./readme.txt", false, b"dot"),
            (b"Materials /x.mat", false, b"space"),
            (b"aux.png", false, b"device"),
            (b"COM1", false, b"port"),
            (b"con .txt", false, b"spaced"),
            (b"...", false, b"dots"),
            (b"COM10.txt", false, b"fine"),
            ("名前:日本語?.png".as_bytes(), true, b"cjk"),
        ]),
    );
    let mut expected: Vec<_> = [
        ("Mac/a_b_.png", &b"colon"[..]),
        ("Mac/a_b__1.png", b"taken"),
        ("Mac/quote_____.txt", b"marks"),
        ("Ver1.0_/readme.txt", b"dot"),
        ("Materials_/x.mat", b"space"),
        ("_aux.png", b"device"),
        ("_COM1", b"port"),
        ("_con .txt", b"spaced"),
        ("___", b"dots"),
        ("COM10.txt", b"fine"),
        ("名前_日本語_.png", b"cjk"),
    ]
    .into_iter()
    .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
    .collect();
    expected.sort();
    assert_eq!(files, expected);
}

#[cfg(windows)]
#[test]
fn bundled_zip_roundtrip_preserves_cp932_utf8_and_ascii_names() {
    let names: &[(&[u8], bool)] = &[
        (b"\x83\x7c.txt", false),
        (b"\x95\x5c.txt", false),
        (b"\x83\x5c.txt", false),
        (b"Readme.txt", false),
        ("髪.txt".as_bytes(), true),
    ];
    let members: Vec<(&[u8], bool, &[u8])> = names
        .iter()
        .map(|(name, utf8)| (*name, *utf8, &b"abc"[..]))
        .collect();
    let files = bundled_extract("authored-zip-names", &authored_zip(&members));
    assert_eq!(files.len(), names.len());
    for name in ["ポ.txt", "表.txt", "ソ.txt", "Readme.txt", "髪.txt"] {
        assert!(
            files.contains(&(name.to_owned(), b"abc".to_vec())),
            "{name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn bundled_locked_archives_open_only_with_their_password() {
    let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tools/7zip/7z.exe");
    let root = crate::test_dir::TestDir::new("archive-password");
    let source = root.join("source/衣装");
    let staging = root.join("staging");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir(&staging).unwrap();
    fs::write(source.join("readme.txt"), b"authored readme").unwrap();
    fs::write(source.join("model.unitypackage"), b"authored package").unwrap();
    let cancelled = AtomicBool::new(false);
    // 7-Zip's command line cannot carry a quote; its input can. ZIP passwords are ASCII only,
    // while 7z takes any text and, with -mhe, hides even the names without it.
    for (name, kind, password) in [
        ("locked.zip", &["-tzip"][..], r#"Authored "quoted" pass \"#),
        (
            "locked.7z",
            &["-t7z", "-mhe=on"][..],
            "合成パスワード \"密碼\"",
        ),
    ] {
        let archive = root.join(name);
        let mut args: Vec<OsString> = ["a", "-p", "-y", "-bd", "-sccUTF-8"]
            .iter()
            .chain(kind)
            .map(OsString::from)
            .collect();
        args.extend([
            archive.as_os_str().to_owned(),
            source.as_os_str().to_owned(),
        ]);
        run_7zip(&executable, &args, &cancelled, false, None, Some(password)).unwrap();
        let bytes = fs::read(&archive).unwrap();
        let extract = |password| {
            extract_archive(
                &executable,
                &archive,
                &[],
                &staging,
                &cancelled,
                &ExtractionProgress::default(),
                password,
            )
        };
        let code = |result: Result<ArchiveExtraction>| result.unwrap_err().to_string();
        assert_eq!(code(extract(None)), "archive_password_required", "{name}");
        assert_eq!(
            code(extract(Some("authored wrong"))),
            "archive_password_wrong",
            "{name}"
        );
        let extracted = extract(Some(password)).unwrap();
        let files: Vec<_> = extracted
            .inventory
            .iter()
            .filter(|entry| !entry.is_directory)
            .map(|entry| {
                let bytes = fs::read(extracted.staging_path.join(&entry.relative_path)).unwrap();
                (entry.relative_path.as_str(), bytes)
            })
            .collect();
        assert_eq!(
            files,
            [
                ("衣装/model.unitypackage", b"authored package".to_vec()),
                ("衣装/readme.txt", b"authored readme".to_vec())
            ],
            "{name}"
        );
        drop(extracted);
        assert_eq!(fs::read(&archive).unwrap(), bytes);
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    }
}

#[test]
#[ignore = "requires explicit NNA_7ZIP pointing to the lead-supplied runtime"]
fn real_7zip_roundtrip_split_password_corruption_and_retention() {
    let executable = PathBuf::from(std::env::var_os("NNA_7ZIP").expect("NNA_7ZIP required"));
    let root = std::env::temp_dir().join(format!("nna-archive-cli-test-{}", uuid::Uuid::new_v4()));
    let source = root.join("source");
    let staging = root.join("staging");
    fs::create_dir_all(source.join("衣装/empty")).unwrap();
    fs::create_dir(&staging).unwrap();
    fs::write(
        source.join("衣装/nested.zip"),
        b"nested archive bytes stay intact",
    )
    .unwrap();
    fs::write(
        source.join("衣装/package.unitypackage"),
        b"unitypackage stays intact",
    )
    .unwrap();
    let bytes: Vec<_> = (0..20000).map(|index| (index % 251) as u8).collect();
    fs::write(source.join("衣装/content.bin"), &bytes).unwrap();
    let cancelled = AtomicBool::new(false);
    let create = |target: &Path, extra: &[&str]| {
        let mut args: Vec<OsString> = vec![
            "a".into(),
            "-tzip".into(),
            "-mx0".into(),
            "-y".into(),
            "-bd".into(),
        ];
        args.extend(extra.iter().map(OsString::from));
        args.push(target.as_os_str().to_owned());
        args.push(source.join("衣装").as_os_str().to_owned());
        run_7zip(&executable, &args, &cancelled, false, None, None).unwrap();
    };
    let zip = root.join("complete.zip");
    create(&zip, &[]);
    let result = extract_archive(
        &executable,
        &zip,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert!(!result
        .staging_path
        .parent()
        .unwrap()
        .join("sources")
        .exists());
    assert!(result
        .inventory
        .iter()
        .any(|entry| entry.is_directory && entry.relative_path.ends_with("empty")));
    assert_eq!(
        fs::read(result.staging_path.join("衣装/content.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        fs::read(result.staging_path.join("衣装/nested.zip")).unwrap(),
        b"nested archive bytes stay intact"
    );
    assert!(result
        .staging_path
        .join("衣装/package.unitypackage")
        .is_file());
    assert!(zip.is_file());

    let seven_archive = root.join("complete.7z");
    run_7zip(
        &executable,
        &[
            "a".into(),
            "-t7z".into(),
            "-y".into(),
            "-bd".into(),
            seven_archive.as_os_str().to_owned(),
            source.join("衣装").as_os_str().to_owned(),
        ],
        &cancelled,
        false,
        None,
        None,
    )
    .unwrap();
    let seven_result = extract_archive(
        &executable,
        &seven_archive,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(seven_result.staging_path.join("衣装/content.bin")).unwrap(),
        bytes
    );
    let rar = root.join("complete.rar");
    fs::write(&rar, tiny_rar_fixture("衣装/synthetic.txt", None)).unwrap();
    assert!(!inspect_password(
        &executable,
        &rar,
        &[],
        &root.join("manual-probe"),
        &cancelled,
        None
    )
    .unwrap());
    assert!(rar.is_file());
    let rar_result = extract_archive(
        &executable,
        &rar,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(rar_result.staging_path.join("衣装/synthetic.txt")).unwrap(),
        b"abc"
    );
    let unsafe_rar = root.join("unsafe.rar");
    fs::write(&unsafe_rar, tiny_rar_fixture("../escape.txt", None)).unwrap();
    assert!(extract_archive(
        &executable,
        &unsafe_rar,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert!(!root.join("escape.txt").exists());
    assert!(unsafe_rar.is_file());
    let ambiguous_rar = root.join("ambiguous.rar");
    fs::write(
        &ambiguous_rar,
        tiny_rar_fixture("safe\nMode = /../../escape.txt", None),
    )
    .unwrap();
    assert!(extract_archive(
        &executable,
        &ambiguous_rar,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert!(!staging.join("escape.txt").exists());
    assert!(!root.join("escape.txt").exists());
    let rar_parts = vec![
        root.join("synthetic.part01.rar"),
        root.join("synthetic.part02.rar"),
    ];
    for (index, path) in rar_parts.iter().enumerate() {
        fs::write(
            path,
            tiny_rar_fixture("衣装/synthetic.txt", Some(index as u8)),
        )
        .unwrap();
    }
    let rar_split_result = extract_archive(
        &executable,
        &rar_parts[1],
        &rar_parts,
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(rar_split_result.staging_path.join("衣装/synthetic.txt")).unwrap(),
        b"abc"
    );
    assert!(rar_parts.iter().all(|path| path.is_file()));

    let split = root.join("split.zip");
    create(&split, &["-v4k"]);
    let mut parts: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("split.zip.")
        })
        .collect();
    parts.sort();
    assert!(parts.len() > 1);
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    let moved = other.join(parts[1].file_name().unwrap());
    fs::rename(&parts[1], &moved).unwrap();
    parts[1] = moved;
    let result = extract_archive(
        &executable,
        &parts[0],
        &parts,
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(result.staging_path.join("衣装/content.bin")).unwrap(),
        bytes
    );
    assert!(parts.iter().all(|path| path.is_file()));
    // A read-only source cannot be aliased safely for cleanup; use the
    // verified-copy fallback without changing the source's attributes.
    let readonly_part = &parts[1];
    let mut permissions = fs::metadata(readonly_part).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(readonly_part, permissions).unwrap();
    let readonly_result = extract_archive(
        &executable,
        &parts[0],
        &parts,
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(readonly_result.staging_path.join("衣装/content.bin")).unwrap(),
        bytes
    );
    assert!(fs::metadata(readonly_part)
        .unwrap()
        .permissions()
        .readonly());
    let mut permissions = fs::metadata(readonly_part).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(readonly_part, permissions).unwrap();
    let completed_staging_count = fs::read_dir(&staging).unwrap().count();
    let incomplete = parts[..parts.len() - 1].to_vec();
    assert!(extract_archive(
        &executable,
        &parts[0],
        &incomplete,
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert_eq!(
        fs::read_dir(&staging).unwrap().count(),
        completed_staging_count
    );

    let password = root.join("password.zip");
    create(&password, &["-pSyntheticOnlyPassword"]);
    assert!(extract_archive(
        &executable,
        &password,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert!(password.is_file());
    let damaged = root.join("damaged.zip");
    fs::write(&damaged, b"independently authored invalid archive").unwrap();
    assert!(extract_archive(
        &executable,
        &damaged,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert!(damaged.is_file());
    assert_eq!(
        fs::read_dir(&staging).unwrap().count(),
        completed_staging_count
    );
    cancelled.store(true, Ordering::Relaxed);
    assert!(extract_archive(
        &executable,
        &zip,
        &[],
        &staging,
        &cancelled,
        &ExtractionProgress::default(),
        None
    )
    .is_err());
    assert!(zip.is_file());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn extraction_output_reads_progress_and_keeps_only_messages() {
    // 7-Zip's own shape: each progress line after a carriage return, then a blank one
    // clearing it.
    let output = b"\r\n7-Zip (x64)\r\n\r\nScanning the drive for archives:\r\n  0M Scan C:\\authored\\\r          \r1 file, 30 bytes\r\n  0%\r    \r 58% 1 - wrong password.txt\r                          \r 97% 2 - authored.bin\r                 \rEverything is Ok\r\n\r\nFiles: 3\r\n";
    let overflow = Arc::new(AtomicBool::new(false));
    let percent = AtomicU64::new(0);
    let kept = read_extraction_output(&output[..], 1024, overflow.clone(), &percent).unwrap();
    let kept = String::from_utf8(kept).unwrap();
    assert_eq!(percent.load(Ordering::Relaxed), 97);
    assert!([
        "7-Zip (x64)",
        "1 file, 30 bytes",
        "Everything is Ok",
        "Files: 3"
    ]
    .iter()
    .all(|message| kept.contains(message)));
    // No progress line stays, so no file name it carries can read as an error.
    assert!(!kept.contains("password") && !kept.contains("authored") && !kept.contains('%'));
    // Hours of progress never fill what is kept.
    let long: Vec<u8> = (0..100_000)
        .flat_map(|step| {
            format!("{:3}% 1 - authored.bin\r                    \r", step % 100).into_bytes()
        })
        .collect();
    let kept = read_extraction_output(&long[..], 1024, overflow.clone(), &percent).unwrap();
    assert!(kept.is_empty() && !overflow.load(Ordering::Relaxed));
    assert_eq!(percent.load(Ordering::Relaxed), 99);
}

#[test]
fn mark_d_archive_policy_includes_split_zip_and_keeps_rar_and_seven_zip_manual() {
    for name in ["sample.ZIP", "sample.zip.001", "sample.z01"] {
        assert!(auto_extract(Path::new(name)));
    }
    for name in [
        "sample.rar",
        "sample.r00",
        "sample.part2.rar",
        "sample.7z",
        "sample.7z.001",
        "sample.unitypackage",
    ] {
        assert!(!auto_extract(Path::new(name)));
    }
}
