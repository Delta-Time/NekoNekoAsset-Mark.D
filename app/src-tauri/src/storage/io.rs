use anyhow::{bail, ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        LazyLock, Mutex,
    },
};

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    ensure_regular(path)?;
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}
pub fn key(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
/// How much of a title or file name a new managed folder name keeps, in UTF-16 units, before
/// `--` and 12 characters of its identity. Folders already made keep their recorded names.
pub const READABLE_UNITS: usize = 32;
/// What 0.4.0 and earlier kept, for an extraction an update interrupted before recording it.
pub const EARLIER_READABLE_UNITS: usize = 64;
pub fn readable_component(label: &str, identity: &str) -> String {
    readable_component_of(label, identity, READABLE_UNITS)
}
pub fn readable_component_of(label: &str, identity: &str, units: usize) -> String {
    let mut name = String::new();
    for ch in label.chars() {
        if name.encode_utf16().count() + ch.len_utf16() > units {
            break;
        }
        name.push(if ch.is_control() || "<>:\"/\\|?*".contains(ch) {
            '_'
        } else {
            ch
        });
    }
    let name = name.trim().trim_end_matches(['.', ' ']);
    let mut component = format!(
        "{}--{}",
        if name.is_empty() { "Asset" } else { name },
        &key(identity)[..12]
    );
    if super::archive::validate_relative_path(&component).is_err() {
        component.insert(0, '_');
    }
    component
}
/// A store may name a file in a way Windows cannot store. Its managed copy uses a readable
/// equivalent: reserved characters become their full-width forms, control characters `_`,
/// trailing dots and spaces are dropped as Windows itself does, and a device name such as
/// CON gains a leading `_`. Valid names are unchanged and a name always maps the same way,
/// so downloads keep their paths; the catalogue keeps the store's own name for display.
pub fn managed_name(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| match c {
            '<' => '＜',
            '>' => '＞',
            ':' => '：',
            '"' => '＂',
            '/' => '／',
            '\\' => '＼',
            '|' => '｜',
            '?' => '？',
            '*' => '＊',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut mapped = mapped.trim_end_matches(['.', ' ']).to_owned();
    if mapped.is_empty() || super::archive::reserved_device(&mapped) {
        mapped.insert(0, '_');
    }
    mapped
}
pub fn unused_relative(root: &Path, base: &str) -> Result<String> {
    for index in 0..10_000 {
        let relative = if index == 0 {
            base.to_owned()
        } else {
            format!("{base}-{index}")
        };
        let path = safe_join(root, &relative)?;
        if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            return Ok(relative);
        }
    }
    bail!("archive_path_collision")
}
/// Antivirus and search indexers briefly open files that were just written, and Windows then
/// refuses to move or remove them (sharing or lock violation, or access denied while the handle
/// is open). Retry such a move or removal for about ten seconds before reporting its error.
pub fn retry_busy<T>(mut operation: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut wait = std::time::Duration::from_millis(50);
    loop {
        match operation() {
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && std::time::Instant::now() + wait < deadline =>
            {
                std::thread::sleep(wait);
                wait = (wait * 2).min(std::time::Duration::from_secs(1));
            }
            result => return result,
        }
    }
}
/// This guard owns only the exact per-file temporary, never published content.
pub struct PartialGuard {
    path: PathBuf,
    parent_key: Option<PathBuf>,
}
static PARTIAL_PARENTS: LazyLock<Mutex<BTreeMap<PathBuf, usize>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
impl PartialGuard {
    pub fn new(path: PathBuf) -> Self {
        let parent_key = path.parent().and_then(|parent| {
            // Callers share the same checked root/path spelling. Keep the key
            // stable even when the parent has not been created yet.
            let key = parent.to_path_buf();
            let mut parents = PARTIAL_PARENTS.lock().ok()?;
            *parents.entry(key.clone()).or_default() += 1;
            Some(key)
        });
        Self { path, parent_key }
    }
}
pub struct EmptyDirectoryGuard(pub PathBuf);
impl Drop for EmptyDirectoryGuard {
    fn drop(&mut self) {
        let Ok(parents) = PARTIAL_PARENTS.lock() else {
            return;
        };
        if parents.contains_key(&self.0) {
            return;
        }
        if super::archive::validate_no_links(&self.0).is_ok() {
            let _ = fs::remove_dir(&self.0);
        }
    }
}
impl Drop for PartialGuard {
    fn drop(&mut self) {
        if super::archive::validate_no_links(&self.path).is_ok()
            && ensure_regular(&self.path).is_ok()
        {
            let _ = fs::remove_file(&self.path);
        }
        if let Some(key) = &self.parent_key {
            if let Ok(mut parents) = PARTIAL_PARENTS.lock() {
                let Some(count) = parents.get_mut(key) else {
                    return;
                };
                *count -= 1;
                if *count == 0 {
                    parents.remove(key);
                    if let Some(parent) = self.path.parent() {
                        if super::archive::validate_no_links(parent).is_ok() {
                            let _ = fs::remove_dir(parent);
                        }
                    }
                }
            }
        }
    }
}
/// Empty-only pruning stops before the checked managed boundary. A sibling,
/// reparse point or user-created file stops cleanup without touching its bytes.
pub fn remove_empty_parents(path: &Path, boundary: &Path) {
    if !path.is_absolute() || !boundary.is_absolute() || !path.starts_with(boundary) {
        return;
    }
    let Ok(relative) = path.strip_prefix(boundary) else {
        return;
    };
    if super::archive::validate_relative_path(&relative.to_string_lossy()).is_err() {
        return;
    }
    let mut current = path;
    while current != boundary && current.starts_with(boundary) {
        if super::archive::validate_no_links(current).is_err() || fs::remove_dir(current).is_err() {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
}
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Acquire), "cancelled");
    Ok(())
}
pub async fn cancellable<T>(
    cancel: &AtomicBool,
    future: impl std::future::Future<Output = T>,
) -> Result<T> {
    check_cancel(cancel)?;
    tokio::select! {result=future=>Ok(result),_ = async {loop{tokio::time::sleep(std::time::Duration::from_millis(100)).await;if cancel.load(Ordering::Acquire){break}}}=>bail!("cancelled")}
}
pub async fn delay(cancel: &AtomicBool, duration: std::time::Duration) -> Result<()> {
    cancellable(cancel, tokio::time::sleep(duration)).await
}
pub fn ensure_regular(path: &Path) -> Result<()> {
    reject_link(path)?;
    ensure!(fs::metadata(path)?.is_file(), "file_missing");
    Ok(())
}
pub fn reject_link(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(!meta.file_type().is_symlink(), "unsafe_link");
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(meta.file_attributes() & 0x400 == 0, "unsafe_link");
    }
    Ok(())
}
/// Check every existing ancestor, including Windows junctions, before accessing managed content.
pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(root.is_absolute(), "root_not_absolute");
    let relative = super::archive::validate_relative_path(relative)?;
    for p in root.ancestors() {
        if p.exists() {
            reject_link(p)?;
        }
    }
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        if fs::symlink_metadata(&path).is_ok() {
            reject_link(&path)?;
        }
    }
    Ok(path)
}
pub fn verify(path: &Path, hash: &str, size: u64) -> Result<()> {
    let actual = hash_file(path)?;
    ensure!(
        actual.0 == hash && actual.1 == size,
        "file_integrity_failed"
    );
    Ok(())
}
pub fn copy_new_verified(
    source: &Path,
    target: &Path,
    hash: &str,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure_regular(source)?;
    let mut input = File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let copied = (|| -> Result<()> {
        let mut buffer = [0u8; 128 * 1024];
        loop {
            check_cancel(cancel)?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
        verify(target, hash, size)
    })();
    drop(output);
    if copied.is_err() {
        let _ = fs::remove_file(target);
    }
    copied
}
pub fn space(path: &Path, required: u64) -> Result<()> {
    ensure!(
        fs2::available_space(path)? >= required.saturating_add(16 * 1024 * 1024),
        "low_space"
    );
    Ok(())
}
/// Total size of the files below `root`, as Explorer's folder Properties reports it. Links
/// and junctions are neither followed nor counted; entries removed by concurrent work are skipped.
pub fn tree_bytes(root: &Path) -> Result<u64> {
    let mut total = 0;
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let items = match fs::read_dir(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && directory != root => {
                continue
            }
            items => items?,
        };
        for item in items {
            let item = item?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                pending.push(item.path());
            } else if kind.is_file() {
                match item.metadata() {
                    Ok(metadata) => total += metadata.len(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(total)
}
pub fn absolute_dir(path: &Path, create: bool) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "root_not_absolute");
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            reject_link(ancestor)?;
        }
    }
    if create {
        fs::create_dir_all(path)?;
    }
    ensure!(path.is_dir(), "root_offline");
    let path = path.canonicalize()?;
    let probe = path.join(format!(".nna-probe-{}", uuid::Uuid::new_v4()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    file.sync_all()?;
    drop(file);
    fs::remove_file(probe)?;
    Ok(path)
}
/// The stable code of an error, which jobs record and the UI translates. A store's error
/// carries the class it was given where it left the store (`stores::DownloadError`); storage's
/// own errors are code constants, matched whole; anything else is read by its io error kind.
pub fn stable_error(err: &anyhow::Error) -> &'static str {
    if let Some(store) = err.downcast_ref::<crate::stores::StoreError>() {
        if store.class != crate::stores::DownloadError::Failed {
            return store.class.code();
        }
    }
    if let Some(code) = raised_code(&err.to_string()) {
        return code;
    }
    if let Some(e) = err.downcast_ref::<std::io::Error>() {
        // Another program or a scanner still held the file after the retries.
        if cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)) {
            return "file_in_use";
        }
        // The drive or network share holding the library went away.
        if cfg!(windows) && matches!(e.raw_os_error(), Some(21 | 53 | 59 | 64 | 67 | 1231)) {
            return "root_offline";
        }
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return "permission_denied";
        }
        if e.kind() == std::io::ErrorKind::StorageFull
            || matches!(e.raw_os_error(), Some(28 | 39 | 112))
        {
            return "low_space";
        }
    }
    "storage_failed"
}

/// The codes storage records as they were raised.
const CODES: [&str; 41] = [
    "cancelled",
    "storage_busy",
    "download_delete_incomplete",
    "root_offline",
    "low_space",
    "auth_required",
    "permission_denied",
    "storage_source_locked",
    "link_expired",
    "rate_limited",
    "network_failed",
    "network_timeout",
    "source_changed",
    "source_tree_incomplete",
    "download_size_mismatch",
    "download_resolver_failed",
    "download_http_failed",
    "manifest_changed",
    "manifest_incomplete",
    "file_missing",
    "file_integrity_failed",
    "archive_password_required",
    "archive_password_wrong",
    "archive_missing_volume",
    "archive_corrupt",
    "archive_listing_invalid",
    "archive_inventory_mismatch",
    "archive_unsafe_path",
    "archive_unsafe_link",
    "archive_path_collision",
    "archive_tool_unavailable",
    "revision_unknown",
    "unsupported_backup_version",
    "backup_payload_missing",
    "destination_not_empty",
    "destination_not_fresh",
    "local_delete_unsafe_path",
    "local_delete_shared_path",
    "local_delete_cleanup_failed",
    "local_delete_rollback_failed",
    "local_delete_retry_required",
];

/// A code storage raised, matched whole: one of `CODES`, or another name storage raises for
/// one of them.
fn raised_code(raised: &str) -> Option<&'static str> {
    Some(match raised {
        "archive_link_forbidden" | "storage_link_forbidden" => "archive_unsafe_link",
        "download_failed" => "download_http_failed",
        "storage.backup_payload_missing" => "backup_payload_missing",
        "storage.cancelled" => "cancelled",
        "storage.destination_not_fresh" => "destination_not_fresh",
        "storage.file_missing" => "file_missing",
        "storage.low_space" | "storage_low_space" => "low_space",
        "storage.source_changed" | "storage_source_changed" | "local_source_changed" => {
            "source_changed"
        }
        _ => return CODES.into_iter().find(|code| *code == raised),
    })
}
pub fn open_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "root_not_absolute");
    ensure!(path.exists(), "file_missing");
    super::archive::validate_no_links(path)?;
    #[cfg(windows)]
    {
        // Filesystem verification keeps verbatim paths; the Shell receives a DOS/UNC path.
        let launch_path = windows_shell_path(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("shell_path_unsupported"))?,
        )?;
        windows_shell_open(launch_path)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("--")
            .arg(path)
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}
#[cfg(windows)]
fn windows_shell_open(path: String) -> Result<()> {
    // A fresh thread guarantees STA even if the blocking worker already uses MTA.
    std::thread::Builder::new()
        .name("nna-shell-open".into())
        .spawn(move || -> Result<()> {
            use windows::{
                core::{w, PCWSTR},
                Win32::{
                    System::Com::{
                        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
                        COINIT_DISABLE_OLE1DDE,
                    },
                    UI::{
                        Shell::{
                            ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC,
                            SHELLEXECUTEINFOW,
                        },
                        WindowsAndMessaging::SW_SHOWNORMAL,
                    },
                },
            };
            // Every successful initialization, including S_FALSE, must be balanced.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
                .ok()
                .map_err(|e| anyhow::anyhow!("shell_com_initialization_failed: {}", e.code()))?;
            struct ComApartment;
            impl Drop for ComApartment {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() };
                }
            }
            let _apartment = ComApartment;
            let target: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                // Complete dispatch before this STA exits; report errors through IPC.
                fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
                lpVerb: w!("open"),
                lpFile: PCWSTR(target.as_ptr()),
                lpParameters: PCWSTR::null(),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            // The target buffer lives through the synchronous call. No process handle
            // is requested, so only the COM initialization needs cleanup.
            unsafe { ShellExecuteExW(&mut info) }
                .map_err(|e| anyhow::anyhow!("shell_open_failed: {}", e.code()))?;
            Ok(())
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("shell_open_failed"))?
}
#[cfg(any(windows, test))]
fn windows_shell_path(path: &str) -> Result<String> {
    ensure!(!path.contains('\0'), "shell_path_unsupported");
    ensure!(!path.starts_with("\\\\.\\"), "shell_path_unsupported");
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("\\\\?\\UNC\\"))
    {
        let suffix = &path[8..];
        let mut parts = suffix.split('\\');
        ensure!(
            parts.next().is_some_and(|s| !s.is_empty())
                && parts.next().is_some_and(|s| !s.is_empty()),
            "shell_path_unsupported"
        );
        return Ok(format!("\\\\{suffix}"));
    }
    if let Some(suffix) = path.strip_prefix("\\\\?\\") {
        let bytes = suffix.as_bytes();
        ensure!(
            bytes.first().is_some_and(u8::is_ascii_alphabetic)
                && bytes.get(1) == Some(&b':')
                && bytes.get(2) == Some(&b'\\'),
            "shell_path_unsupported"
        );
        return Ok(suffix.to_owned());
    }
    Ok(path.to_owned())
}
pub fn required_string<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    match value[field].as_str() {
        Some(s) if !s.is_empty() => Ok(s),
        _ => bail!("invalid_arguments"),
    }
}

#[cfg(test)]
#[path = "io_launch_path_tests.rs"]
mod launch_path_tests;
