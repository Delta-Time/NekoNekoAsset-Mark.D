//! Temporary folders that tests remove themselves.
use std::path::{Path, PathBuf};

/// `nna-<name>-<uuid>` in the system temp folder, deleted with its contents when this
/// drops, after a failed assertion too. Nothing is created until the test does.
/// Windows refuses to delete a file SQLite still holds open, so bind this before the
/// catalogue or service that opens files inside it (locals drop in reverse order), or
/// make it the last field of a fixture that holds them.
pub(crate) struct TestDir(PathBuf);

impl TestDir {
    pub(crate) fn new(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!("nna-{name}-{}", uuid::Uuid::new_v4())))
    }
}

impl std::ops::Deref for TestDir {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
