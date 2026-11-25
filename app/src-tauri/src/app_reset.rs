//! Explicit current-session reset, applied only after the old process releases its locks.
use crate::{catalogue::Catalogue, library_lock::LibraryLock, profile, storage};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const REQUEST: &str = "app-reset.json";
pub(crate) const DELETE_MARKER: &str = ".nna-reset-delete";
const LOCK: &str = ".nna-library.lock";
pub(crate) fn pending(device: &Path) -> bool {
    device.join(REQUEST).exists()
}
pub(crate) fn pending_library(device: &Path) -> Option<PathBuf> {
    serde_json::from_slice::<Request>(&fs::read(device.join(REQUEST)).ok()?)
        .ok()
        .map(|request| request.library)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Stage {
    Scheduled,
    Deleting,
    FinalCleanup,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    library: PathBuf,
    delete_library: bool,
    operation: String,
    root_id: String,
    stage: Stage,
}
fn same_path(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (a.canonicalize(), b.canonicalize()) {
        return a == b;
    }
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
            == b.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase()
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn checked_root(root: &Path, device: &Path) -> Result<PathBuf> {
    let root = storage::checked_library_root(root)?;
    let device = device.canonicalize()?;
    let executable = tauri::utils::platform::current_exe()?.canonicalize()?;
    ensure!(
        root.parent().is_some()
            && !device.starts_with(&root)
            && !root.starts_with(&device)
            && !executable.starts_with(&root),
        "reset_unsafe_location"
    );
    Ok(root)
}
fn tree(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(path) = pending.pop() {
        ensure!(path.starts_with(root), "reset_unsafe_location");
        storage::validate_library_path(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_dir() || metadata.is_file(),
            "reset_unsafe_location"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        }
        paths.push(path);
    }
    Ok(paths)
}
pub(crate) fn schedule(device: &Path, catalogue: &Catalogue, delete_library: bool) -> Result<()> {
    let root = checked_root(
        catalogue.library_root().context("library_not_open")?,
        device,
    )?;
    ensure!(
        root.join(".nna/data/catalogue.sqlite3").is_file(),
        "reset_unsafe_location"
    );
    if delete_library {
        tree(&root)?;
    }
    let request = Request {
        library: root,
        delete_library,
        operation: uuid::Uuid::new_v4().to_string(),
        root_id: catalogue
            .get("preferences", "app")?
            .context("catalogue.preferences_missing")?["libraryRootId"]
            .as_str()
            .context("reset_invalid_request")?
            .to_owned(),
        stage: Stage::Scheduled,
    };
    let destination = device.join(REQUEST);
    storage::validate_library_path(&destination)?;
    ensure!(!destination.exists(), "reset_pending");
    write_request(device, &request)
}
fn write_request(device: &Path, request: &Request) -> Result<()> {
    let destination = device.join(REQUEST);
    storage::validate_library_path(&destination)?;
    let temporary = device.join(format!("app-reset-{}.pending", uuid::Uuid::new_v4()));
    storage::validate_library_path(&temporary)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(request)?)?;
        file.sync_all()?;
        drop(file);
        crate::ai::provider::replace_file(&temporary, &destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn write_marker(request: &Request) -> Result<()> {
    let path = request.library.join(DELETE_MARKER);
    storage::validate_library_path(&path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let result = (|| {
        file.write_all(request.operation.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}
fn remove_library(device: &Path, request: &mut Request) -> Result<()> {
    let root = &request.library;
    storage::validate_library_path(&root.join(LOCK))?;
    let lock = LibraryLock::acquire(root)?;
    let marker = root.join(DELETE_MARKER);
    storage::validate_library_path(&marker)?;
    if matches!(request.stage, Stage::Scheduled) {
        // A durable request precedes its root marker. Bind an interrupted first
        // startup to the original database rather than a same-name replacement.
        tree(root)?;
        let database = root.join(".nna/data/catalogue.sqlite3");
        storage::validate_library_path(&database)?;
        let connection = rusqlite::Connection::open_with_flags(
            &database,
            // The prior process may have died with a hot rollback journal.
            // SQLite must recover it before reading the durable library identity.
            // The OS library lock is already held; never create or initialize SQL.
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        ensure!(version == 1, "catalogue.newer_schema");
        let body: String = connection.query_row(
            "SELECT body FROM entities WHERE kind='preferences' AND id='app'",
            [],
            |row| row.get(0),
        )?;
        let preferences: serde_json::Value = serde_json::from_str(&body)?;
        ensure!(
            preferences["libraryRootId"].as_str() == Some(request.root_id.as_str()),
            "reset_invalid_request"
        );
        drop(connection);
        if !marker.exists() {
            write_marker(request)?;
        }
        ensure!(
            fs::read(&marker)? == request.operation.as_bytes(),
            "reset_invalid_request"
        );
        request.stage = Stage::Deleting;
        write_request(device, request)?;
    }
    ensure!(
        fs::read(&marker)? == request.operation.as_bytes(),
        "reset_invalid_request"
    );
    if matches!(request.stage, Stage::Deleting) {
        let paths = tree(root)?; // Validate the complete tree before the first destructive write.
        for path in paths.into_iter().rev() {
            if path == *root || path == root.join(LOCK) || path == marker {
                continue;
            }
            storage::validate_library_path(&path)?;
            if fs::symlink_metadata(&path)?.is_dir() {
                fs::remove_dir(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        request.stage = Stage::FinalCleanup;
        write_request(device, request)?;
    }
    lock.check_available()?;
    drop(lock);
    // The deletion marker makes other Apps refuse SQL initialization after acquiring
    // the lock. Final removals are nonrecursive: new external writes stop cleanup.
    fs::remove_file(root.join(LOCK))?;
    fs::remove_file(marker)?;
    if let Err(error) = fs::remove_dir(root) {
        let _ = write_marker(request);
        return Err(error.into());
    }
    Ok(())
}
pub(crate) fn apply_pending(device: &Path) -> Result<()> {
    let path = device.join(REQUEST);
    storage::validate_library_path(&path)?;
    if !path.exists() {
        return Ok(());
    }
    let mut request: Request = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|_| anyhow::anyhow!("reset_invalid_request"))?;
    ensure!(
        request.library.is_absolute() && uuid::Uuid::parse_str(&request.operation).is_ok(),
        "reset_invalid_request"
    );
    if let Some(selection) = profile::selected_library(device)? {
        // Missing paths are compared lexically so an interrupted final cleanup can retry.
        ensure!(
            same_path(&selection.path, &request.library),
            "reset_invalid_request"
        );
    } else {
        ensure!(
            matches!(request.stage, Stage::FinalCleanup),
            "reset_invalid_request"
        );
    }
    if !request.delete_library && matches!(request.stage, Stage::FinalCleanup) {
        // Settings are already durable; retry only the profile locator cleanup.
    } else if request.library.exists() {
        let root = checked_root(&request.library, device)?;
        if request.delete_library {
            request.library = root;
            remove_library(device, &mut request)?;
        } else {
            ensure!(
                root.join(".nna/data/catalogue.sqlite3").is_file(),
                "reset_invalid_request"
            );
            let catalogue = Catalogue::open_bound(&root, device)?;
            ensure!(
                catalogue
                    .get("preferences", "app")?
                    .context("catalogue.preferences_missing")?["libraryRootId"]
                    .as_str()
                    == Some(request.root_id.as_str()),
                "reset_invalid_request"
            );
            let mut preferences = crate::catalogue::default_preferences();
            preferences["libraryRootId"] = catalogue
                .get("preferences", "app")?
                .context("catalogue.preferences_missing")?["libraryRootId"]
                .clone();
            catalogue.put("preferences", "app", preferences)?;
            catalogue.prepare_sync_document()?;
            request.stage = Stage::FinalCleanup;
            write_request(device, &request)?;
        }
    } else {
        ensure!(
            request.delete_library && matches!(request.stage, Stage::FinalCleanup),
            "root_offline"
        );
    }
    // Protected sessions, provider credentials and chats are deliberately untouched.
    let locator = device.join("library.json");
    storage::validate_library_path(&locator)?;
    if locator.exists() {
        fs::remove_file(locator)?;
    }
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
#[path = "app_reset_tests.rs"]
mod tests;
