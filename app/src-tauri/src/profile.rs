//! Portable packages keep their device profile beside the executable.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::utils::config::AppDirectoriesOverride;

const PORTABLE_MARKER: &str = "portable.flag";
const RESTART_PARENT: &str = "NNA_RESTART_PARENT";
const DEFAULT_LIBRARY_FOLDER: &str = "NekoNekoDatabase";

/// Installed profiles live in the user's local data directory. Keep the library
/// beside the profile, outside both Program Files and the reset-protected profile.
/// Portable profiles are sibling `data/`, so their library stays beside the App.
/// Looking up this candidate never creates or selects a library.
pub(crate) fn default_library_path(device_dir: &Path) -> Result<PathBuf> {
    Ok(device_dir
        .parent()
        .context("library_unavailable")?
        .join(DEFAULT_LIBRARY_FOLDER))
}

fn library_subfolder(parent: &Path) -> PathBuf {
    let named = parent.file_name().and_then(|name| name.to_str());
    if named.is_some_and(|name| name.eq_ignore_ascii_case(DEFAULT_LIBRARY_FOLDER)) {
        parent.to_path_buf()
    } else {
        parent.join(DEFAULT_LIBRARY_FOLDER)
    }
}

/// A new-library choice is a parent folder, unless it already names the
/// database folder or contains an existing current library.
pub(crate) fn new_library_path(parent: &Path) -> PathBuf {
    if parent
        .join(".nna")
        .join("data")
        .join("catalogue.sqlite3")
        .is_file()
    {
        parent.to_path_buf()
    } else {
        library_subfolder(parent)
    }
}

/// Tauri starts the replacement before exiting. Keep its startup behind the
/// exact old process, whose exit releases the library and profile locks.
pub(crate) fn prepare_restart_handoff() -> Result<()> {
    #[cfg(windows)]
    std::env::set_var(RESTART_PARENT, restart_handoff::marker()?);
    Ok(())
}

fn wait_for_restart_parent() -> Result<()> {
    let Some(marker) = std::env::var_os(RESTART_PARENT) else {
        return Ok(());
    };
    std::env::remove_var(RESTART_PARENT);
    #[cfg(windows)]
    restart_handoff::wait(
        marker
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("library_restart_failed"))?,
    )?;
    #[cfg(not(windows))]
    let _ = marker;
    Ok(())
}

#[cfg(windows)]
mod restart_handoff {
    use super::*;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    #[allow(non_snake_case)]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    struct Process(*mut c_void);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn creation_time(process: *mut c_void) -> Result<u64> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        anyhow::ensure!(
            unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
                != 0,
            "library_restart_failed"
        );
        Ok((u64::from(created.high) << 32) | u64::from(created.low))
    }

    pub(super) fn marker() -> Result<String> {
        Ok(format!(
            "{}:{}",
            std::process::id(),
            creation_time(unsafe { GetCurrentProcess() })?
        ))
    }

    pub(super) fn wait(marker: &str) -> Result<()> {
        let (pid, created) = marker
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("library_restart_failed"))?;
        let pid: u32 = pid
            .parse()
            .map_err(|_| anyhow::anyhow!("library_restart_failed"))?;
        let created: u64 = created
            .parse()
            .map_err(|_| anyhow::anyhow!("library_restart_failed"))?;
        anyhow::ensure!(
            pid != 0 && pid != std::process::id() && created != 0,
            "library_restart_failed"
        );
        // SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION; the handle pins this
        // process identity even if its PID is reused after it exits.
        let handle = unsafe { OpenProcess(0x0010_0000 | 0x1000, 0, pid) };
        if handle.is_null() {
            // ERROR_INVALID_PARAMETER: this process has already exited.
            anyhow::ensure!(unsafe { GetLastError() } == 87, "library_restart_failed");
            return Ok(());
        }
        let process = Process(handle);
        if creation_time(process.0)? != created {
            return Ok(());
        }
        // Bound an invalid handoff, without releasing or stealing any lock.
        anyhow::ensure!(
            unsafe { WaitForSingleObject(process.0, 30_000) } == 0,
            "library_restart_failed"
        );
        Ok(())
    }
}

/// A device remembers which library to open; credentials stay in this profile.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LibrarySelection {
    pub path: PathBuf,
    pub locale: String,
}

pub(crate) fn selected_library(device_dir: &Path) -> Result<Option<LibrarySelection>> {
    let path = device_dir.join("library.json");
    if !path.exists() {
        return Ok(None);
    }
    let selection: LibrarySelection = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|_| anyhow::anyhow!("library_preferences_invalid"))?;
    anyhow::ensure!(selection.path.is_absolute(), "library_preferences_invalid");
    Ok(Some(selection))
}

pub(crate) fn select_library(device_dir: &Path, selection: &LibrarySelection) -> Result<()> {
    let result = (|| -> Result<()> {
        std::fs::create_dir_all(device_dir)?;
        let temporary = device_dir.join("library.json.pending");
        let destination = device_dir.join("library.json");
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&serde_json::to_vec(selection)?)?;
            file.sync_all()?;
        }
        crate::ai::provider::replace_file(&temporary, &destination)?;
        Ok(())
    })();
    result.context("library_preferences_write_failed")
}

fn portable_data_dir(executable: &Path) -> Option<PathBuf> {
    let folder = executable.parent()?;
    folder
        .join(PORTABLE_MARKER)
        .is_file()
        .then(|| folder.join("data"))
}

fn portable_identifier(identifier: &str, data_dir: &Path) -> String {
    let path = data_dir.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let path = path.to_lowercase();
    let digest = Sha256::digest(path.trim_end_matches('/').as_bytes());
    format!("{identifier}.portable.{}", hex::encode(digest))
}

pub fn configure(context: &mut tauri::Context<tauri::Wry>) -> Result<()> {
    wait_for_restart_parent()?;
    #[cfg(debug_assertions)]
    if let Some(device_dir) = std::env::var_os("NNA_DATA_DIR") {
        let config = context.config_mut();
        config.identifier = portable_identifier(&config.identifier, Path::new(&device_dir));
        config.app.app_directories_override =
            Some(AppDirectoriesOverride::Root(PathBuf::from(device_dir)));
        return Ok(());
    }
    let executable =
        tauri::utils::platform::current_exe().context("Executable location unavailable")?;
    if let Some(data_dir) = portable_data_dir(&executable) {
        let config = context.config_mut();
        // Resolve before windows and plugins start, so WebView storage and the
        // single-instance guard follow the same profile as the catalogue.
        config.identifier = portable_identifier(&config.identifier, &data_dir);
        config.app.app_directories_override = Some(AppDirectoriesOverride::Root(data_dir));
    }
    Ok(())
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
