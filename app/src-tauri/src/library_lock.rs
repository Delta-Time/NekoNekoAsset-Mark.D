//! One live App owns a shared library. The OS lock, not file age, is authoritative.

use anyhow::{Context, Result};
use std::{fs::File, fs::OpenOptions, path::Path};

const LOCK_FILE: &str = ".nna-library.lock";

pub(crate) struct LibraryLock {
    file: File,
}

impl LibraryLock {
    /// Acquire before opening the catalogue or changing any library content.
    /// The selected root must already exist; no fallback root is created.
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        Self::acquire_with_create(root, true)
    }

    /// A setup probe must not materialize even a lock file in its candidate.
    #[cfg(test)]
    pub(crate) fn acquire_existing(root: &Path) -> Result<Self> {
        Self::acquire_with_create(root, false)
    }

    fn acquire_with_create(root: &Path, create: bool) -> Result<Self> {
        if !root.is_dir() {
            anyhow::bail!("library_lock_unavailable");
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(create)
            .truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Permit other Apps to attempt the byte-range lock, but prevent
            // deleting/replacing its file while this handle is alive.
            const FILE_SHARE_READ: u32 = 1;
            const FILE_SHARE_WRITE: u32 = 2;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let file = options
            .open(root.join(LOCK_FILE))
            .context("library_lock_unavailable")?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
                anyhow::anyhow!("library_in_use")
            } else {
                anyhow::Error::new(error).context("library_lock_unavailable")
            }
        })?;
        let guard = Self { file };
        guard.check_available()?;
        Ok(guard)
    }

    /// A surfaced network/handle error requires stopping library work. This
    /// never drops the handle, steals a stale lock, or retries acquisition.
    pub(crate) fn check_available(&self) -> Result<()> {
        self.file.sync_all().context("library_lock_unavailable")
    }
}

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
        // Closing the handle also releases the lock after a crash/failed unlock.
        // Leave the stable lock file; deleting it would race another opener.
    }
}

#[cfg(test)]
#[path = "library_lock_tests.rs"]
mod tests;
