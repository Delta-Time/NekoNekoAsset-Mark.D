//! Explicit movement of a current shared library, without upgrading old data.
use super::*;
use crate::library_lock::LibraryLock;
use anyhow::Context;

impl StorageService {
    pub fn move_library(&self, args: &Value, cancel: &AtomicBool) -> Result<Value> {
        let source = self
            .catalogue
            .library_root()
            .ok_or_else(|| anyhow::anyhow!("root_unconfigured"))?
            .to_path_buf();
        let permit = self.catalogue.begin_restore_work()?;
        io::check_cancel(cancel)?;
        let requested = Path::new(io::required_string(args, "path")?);
        ensure!(requested.is_absolute(), "root_not_absolute");
        archive::validate_no_links(requested)?;
        std::fs::create_dir_all(requested).context("storage.destination_unavailable")?;
        let destination = checked_library_root(requested)?;
        ensure!(
            !source.starts_with(&destination) && !destination.starts_with(&source),
            "storage.migration_overlapping_roots"
        );
        // Reject unrelated contents before acquiring a lock can create its marker.
        // copy_into_locked_root rechecks under the destination lock.
        for entry in std::fs::read_dir(&destination).context("storage.destination_unavailable")? {
            ensure!(
                entry
                    .context("storage.destination_unavailable")?
                    .file_name()
                    == ".nna-library.lock",
                "storage.destination_not_fresh"
            );
        }
        let destination_lock = Arc::new(LibraryLock::acquire(&destination)?);
        let mut payload = self.payload(None, None, false)?;
        for item in &mut payload {
            item.relative_path = item
                .source_path
                .strip_prefix(&source)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_path"))?
                .replace('\\', "/");
        }
        // The selected library is user-owned. Preserve additional files and
        // empty folders too, retaining stronger hashes for managed artifacts.
        self.complete_library_payload(&source, &mut payload, cancel)
            .context("storage.read_failed")?;
        let snapshot = self
            .catalogue
            .data_dir()
            .join(format!(".nna-move-db-{}.sqlite3", uuid::Uuid::new_v4()));
        let _snapshot = io::PartialGuard::new(snapshot.clone());
        self.catalogue.snapshot_database(&snapshot)?;
        let (hash, _) = io::hash_file(&snapshot)?;
        // Source snapshot is immutable while copying. It is installed last at
        // the ordinary current-format path, never by copying a live SQLite file.
        payload.push(portable::PayloadSource {
            root_id: "__catalogue".into(),
            relative_path: ".nna/data/catalogue.sqlite3".into(),
            source_path: snapshot,
            expected_hash: Some(hash),
            is_directory: false,
        });
        let result = portable::copy_into_locked_root(&source, &destination, &payload, cancel)?;
        io::check_cancel(cancel)?;
        self.catalogue.retain_move_lock(destination_lock, permit)?;
        Ok(
            json!({"libraryPath":destination,"restartRequired":true,"fileCount":result.file_count,"totalBytes":result.total_bytes}),
        )
    }

    fn complete_library_payload(
        &self,
        library: &Path,
        payload: &mut Vec<portable::PayloadSource>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let mut known: HashMap<_, _> = payload
            .iter()
            .enumerate()
            .map(|(index, item)| (item.relative_path.clone(), index))
            .collect();
        ensure!(known.len() == payload.len(), "archive_path_collision");
        let mut pending = std::fs::read_dir(library)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        while let Some(path) = pending.pop() {
            io::check_cancel(cancel)?;
            let relative = path
                .strip_prefix(library)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_path"))?
                .replace('\\', "/");
            if move_transient(&relative) {
                continue;
            }
            io::reject_link(&path)?;
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_dir() || metadata.is_file(),
                "storage_source_changed"
            );
            let is_directory = metadata.is_dir();
            if let Some(index) = known.get(&relative) {
                ensure!(
                    payload[*index].source_path == path
                        && payload[*index].is_directory == is_directory,
                    "storage_source_changed"
                );
            } else {
                known.insert(relative.clone(), payload.len());
                payload.push(portable::PayloadSource {
                    root_id: "__catalogue".into(),
                    relative_path: relative,
                    source_path: path.clone(),
                    expected_hash: None,
                    is_directory,
                });
            }
            if is_directory {
                for entry in std::fs::read_dir(path)? {
                    pending.push(entry?.path());
                }
            }
        }
        Ok(())
    }
}

fn move_transient(relative: &str) -> bool {
    matches!(
        relative,
        ".nna-library.lock"
            | ".nna/data/storage.lock"
            | ".nna/data/catalogue.sqlite3"
            | ".nna/data/catalogue.sqlite3-journal"
            | ".nna/data/catalogue.sqlite3-wal"
            | ".nna/data/catalogue.sqlite3-shm"
    ) || [".nna-staging", ".nna-partials", ".nna-import-partials"]
        .iter()
        .any(|directory| relative == *directory || relative.starts_with(&format!("{directory}/")))
        || relative
            .strip_prefix(".nna/data/")
            .and_then(|name| name.strip_prefix(".nna-move-db-"))
            .and_then(|name| name.strip_suffix(".sqlite3"))
            .is_some_and(|id| {
                uuid::Uuid::parse_str(id).is_ok_and(|parsed| parsed.to_string() == id)
            })
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
