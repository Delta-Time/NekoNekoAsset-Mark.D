use super::*;

#[test]
fn initial_library_default_uses_the_device_profile_parent_without_creating_content() {
    let authored_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("authored-library-default-{}", uuid::Uuid::new_v4()));
    for folder in ["Local App Data", "Portable App"] {
        let directory = authored_root.join(folder);
        let profile = directory.join("device-profile");
        assert_eq!(
            default_library_path(&profile).unwrap(),
            directory.join("NekoNekoDatabase")
        );
        assert!(!default_library_path(&profile)
            .unwrap()
            .starts_with(&profile));
    }
    assert!(
        !authored_root.exists(),
        "default lookup created a directory"
    );
}

#[test]
#[cfg(windows)]
fn installed_library_default_is_outside_program_files_and_the_device_profile() {
    let profile = Path::new(r"C:\Users\Authored User\AppData\Local\app.nekonekoasset.desktop");
    let library = default_library_path(profile).unwrap();
    assert_eq!(
        library,
        Path::new(r"C:\Users\Authored User\AppData\Local\NekoNekoDatabase")
    );
    assert!(!library.starts_with(profile));
    assert!(!library.starts_with(r"C:\Program Files"));
    assert_eq!(
        default_library_path(Path::new(r"F:\Portable App\data")).unwrap(),
        Path::new(r"F:\Portable App\NekoNekoDatabase")
    );
}

#[cfg(windows)]
#[test]
fn new_library_subfolder_is_added_once_for_drive_and_unc_parents() {
    for (parent, expected) in [
        ("F:\\Assets", "F:\\Assets\\NekoNekoDatabase"),
        ("F:\\Assets\\", "F:\\Assets\\NekoNekoDatabase"),
        ("F:\\", "F:\\NekoNekoDatabase"),
        ("F:/Assets/", "F:/Assets/NekoNekoDatabase"),
        (
            "F:\\Assets\\NekoNekoDatabase",
            "F:\\Assets\\NekoNekoDatabase",
        ),
        (
            "F:\\Assets\\nekonekodatabase\\",
            "F:\\Assets\\nekonekodatabase\\",
        ),
        (
            "\\\\authored-server\\share\\Assets\\",
            "\\\\authored-server\\share\\Assets\\NekoNekoDatabase",
        ),
        (
            "\\\\authored-server\\share\\NekoNekoDatabase\\",
            "\\\\authored-server\\share\\NekoNekoDatabase\\",
        ),
    ] {
        let result = library_subfolder(Path::new(parent));
        assert_eq!(result, Path::new(expected));
        assert_eq!(library_subfolder(&result), result);
    }
}

#[test]
fn new_library_choice_retains_an_existing_catalogue_at_its_exact_root() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "authored-existing-library-{}",
            uuid::Uuid::new_v4()
        ));
    let data = root.join(".nna").join("data");
    std::fs::create_dir_all(&data).unwrap();
    assert_eq!(new_library_path(&root), root.join("NekoNekoDatabase"));
    let marker = data.join("catalogue.sqlite3");
    std::fs::write(&marker, b"authored library marker").unwrap();
    assert_eq!(new_library_path(&root), root);
    assert_eq!(new_library_path(&root).as_os_str(), root.as_os_str());
    assert_eq!(std::fs::read(&marker).unwrap(), b"authored library marker");
    std::fs::remove_file(marker).unwrap();
    std::fs::remove_dir(data).unwrap();
    std::fs::remove_dir(root.join(".nna")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[cfg(windows)]
#[test]
fn restart_handoff_child() {
    use std::io::{Read, Write};
    let Some(mode) = std::env::var_os("NNA_PROFILE_HANDOFF_TEST") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("NNA_PROFILE_HANDOFF_ROOT").unwrap());
    if mode == "owner" {
        let _lock = crate::library_lock::LibraryLock::acquire(&root).unwrap();
        println!("HANDOFF_OWNER:{}", restart_handoff::marker().unwrap());
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_exact(&mut [0]).unwrap();
    } else {
        println!("HANDOFF_WAITING");
        std::io::stdout().flush().unwrap();
        wait_for_restart_parent().unwrap();
        assert!(std::env::var_os(RESTART_PARENT).is_none());
        assert_eq!(
            std::env::var("NNA_DATA_DIR").unwrap(),
            "authored-device-profile"
        );
        let _lock = crate::library_lock::LibraryLock::acquire(&root).unwrap();
    }
}

#[cfg(windows)]
#[test]
fn restart_handoff_waits_for_exact_parent_and_preserves_device_environment() {
    use std::{
        io::{BufRead, BufReader},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("authored-restart-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let spawn = |mode: &str, marker: Option<&str>| {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "profile::tests::restart_handoff_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NNA_PROFILE_HANDOFF_TEST", mode)
            .env("NNA_PROFILE_HANDOFF_ROOT", &root)
            .env("NNA_DATA_DIR", "authored-device-profile")
            .env_remove(RESTART_PARENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000);
        if let Some(marker) = marker {
            command.env(RESTART_PARENT, marker);
        }
        command.spawn().unwrap()
    };
    let read_until = |reader: &mut BufReader<std::process::ChildStdout>, prefix: &str| {
        let mut line = String::new();
        loop {
            assert!(
                reader.read_line(&mut line).unwrap() != 0,
                "authored child ended before {prefix}"
            );
            if let Some(offset) = line.find(prefix) {
                break line[offset + prefix.len()..].trim().to_owned();
            }
            line.clear();
        }
    };
    let mut owner = spawn("owner", None);
    let marker = read_until(
        &mut BufReader::new(owner.stdout.take().unwrap()),
        "HANDOFF_OWNER:",
    );
    let (pid, created) = marker.split_once(':').unwrap();
    // A live unrelated process with a reused PID must never hold startup.
    restart_handoff::wait(&format!("{pid}:{}", created.parse::<u64>().unwrap() + 1)).unwrap();
    assert!(restart_handoff::wait("invalid").is_err());
    assert!(restart_handoff::wait(&restart_handoff::marker().unwrap()).is_err());
    let mut replacement = spawn("replacement", Some(&marker));
    let mut replacement_output = BufReader::new(replacement.stdout.take().unwrap());
    read_until(&mut replacement_output, "HANDOFF_WAITING");
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(
        replacement.try_wait().unwrap().is_none(),
        "replacement passed its still-live parent's gate"
    );
    owner.kill().unwrap();
    owner.wait().unwrap();
    assert!(replacement.wait_with_output().unwrap().status.success());
    std::fs::remove_file(root.join(".nna-library.lock")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn marker_selects_executable_folder_without_reading_or_copying_local_profile() {
    let folder = std::env::temp_dir().join(format!("nna-profile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let executable = folder.join("NekoNekoAsset.exe");
    assert_eq!(portable_data_dir(&executable), None);
    std::fs::write(folder.join(PORTABLE_MARKER), "").unwrap();
    assert_eq!(portable_data_dir(&executable), Some(folder.join("data")));
    assert!(!folder.join("data").exists());
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn portable_copies_have_independent_instance_ids() {
    let base = "app.nekonekoasset.desktop";
    let first = portable_identifier(base, Path::new("F:/Portable/One/data"));
    assert_ne!(first, base);
    assert_ne!(
        first,
        portable_identifier(base, Path::new("F:/Portable/Two/data"))
    );
    #[cfg(windows)]
    assert_eq!(
        first,
        portable_identifier(base, Path::new("f:\\portable\\one\\data\\"))
    );
}

#[test]
fn library_selection_replaces_only_the_device_locator() {
    let directory = std::env::temp_dir().join(format!("nna-locator-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("session-sentinel"), b"device-owned").unwrap();
    assert!(selected_library(&directory).unwrap().is_none());
    for (name, locale) in [("first", "en"), ("second", "zh-Hant")] {
        let path = directory.join(name);
        select_library(
            &directory,
            &LibrarySelection {
                path: path.clone(),
                locale: locale.into(),
            },
        )
        .unwrap();
        let selected = selected_library(&directory).unwrap().unwrap();
        assert_eq!(selected.path, path);
        assert_eq!(selected.locale, locale);
    }
    assert_eq!(
        std::fs::read(directory.join("session-sentinel")).unwrap(),
        b"device-owned"
    );
    std::fs::write(
        directory.join("library.json"),
        br#"{"path":"relative","locale":"en"}"#,
    )
    .unwrap();
    assert!(selected_library(&directory).is_err());
    std::fs::remove_dir_all(&directory).unwrap();
}
