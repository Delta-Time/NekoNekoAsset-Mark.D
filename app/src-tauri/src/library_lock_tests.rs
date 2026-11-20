use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct TestRoot(PathBuf);
impl TestRoot {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("library-lock-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join(LOCK_FILE));
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn child(root: &Path, mode: &str) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "library_lock::tests::lock_child_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NNA_LOCK_TEST_ROOT", root)
        .env("NNA_LOCK_TEST_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.spawn().unwrap()
}

#[test]
fn lock_child_process() {
    let Some(root) = std::env::var_os("NNA_LOCK_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    if std::env::var("NNA_LOCK_TEST_MODE").unwrap() == "reject" {
        assert_eq!(
            LibraryLock::acquire(&root).err().unwrap().to_string(),
            "library_in_use"
        );
        return;
    }
    let _guard = LibraryLock::acquire(&root).unwrap();
    println!("LIBRARY_LOCK_READY");
    std::io::stdout().flush().unwrap();
    let mut exit = [0];
    std::io::stdin().read_exact(&mut exit).unwrap();
}

#[test]
fn second_process_is_rejected_until_owner_closes() {
    let root = TestRoot::new();
    let guard = LibraryLock::acquire(&root.0).unwrap();
    let denied = child(&root.0.join("."), "reject")
        .wait_with_output()
        .unwrap();
    assert!(denied.status.success());
    guard.check_available().unwrap();
    drop(guard);
    LibraryLock::acquire(&root.0).unwrap();
}

#[test]
fn process_crash_releases_lock_without_stale_file_cleanup() {
    let root = TestRoot::new();
    let mut owner = child(&root.0, "hold");
    let mut output = BufReader::new(owner.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        assert!(output.read_line(&mut line).unwrap() != 0);
        if line.contains("LIBRARY_LOCK_READY") {
            break;
        }
        line.clear();
    }
    assert_eq!(
        LibraryLock::acquire(&root.0).err().unwrap().to_string(),
        "library_in_use"
    );
    owner.kill().unwrap();
    owner.wait().unwrap();
    assert!(root.0.join(LOCK_FILE).is_file());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match LibraryLock::acquire(&root.0) {
            Ok(_) => break,
            Err(error) if error.to_string() == "library_in_use" => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

#[cfg(windows)]
#[test]
fn active_lock_file_cannot_be_replaced() {
    let root = TestRoot::new();
    let guard = LibraryLock::acquire(&root.0).unwrap();
    assert!(std::fs::remove_file(root.0.join(LOCK_FILE)).is_err());
    drop(guard);
    assert!(root.0.join(LOCK_FILE).is_file());
    LibraryLock::acquire(&root.0).unwrap();
}
