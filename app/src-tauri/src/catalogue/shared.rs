//! A sequentially shared library keeps catalogue facts and relative content together.
use super::*;
use crate::library_lock::LibraryLock;
use sha2::{Digest, Sha256};

pub(super) fn device_id(profile: &Path) -> Result<String> {
    let profile = profile.canonicalize()?;
    let machine = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    Ok(hex::encode(Sha256::digest(
        format!("{machine}\0{}", profile.to_string_lossy()).as_bytes(),
    )))
}

fn setup_probe_uri(database: &Path) -> Result<String> {
    let mut uri = url::Url::from_file_path(database)
        .map_err(|_| anyhow::anyhow!("root_not_absolute"))?;
    uri.query_pairs_mut().append_pair("immutable", "1");
    if uri.has_host() {
        // SQLite rejects a non-local URI authority. Put the UNC server in
        // the path, so SQLite decodes it as //server/share/...
        Ok(uri.as_str().replacen("file://", "file:////", 1))
    } else {
        Ok(uri.into())
    }
}

#[cfg(all(test, windows))]
#[test]
fn setup_probe_uri_accepts_unc_without_a_sqlite_uri_authority() {
    let mut uri = setup_probe_uri(Path::new(
        r"\\?\UNC\nna-probe\share\catalogue #%.sqlite3",
    ))
    .unwrap();
    assert_eq!(
        uri,
        "file:////nna-probe/share/catalogue%20%23%25.sqlite3?immutable=1"
    );
    // Exercise SQLite's URI parser without opening a network connection.
    uri.push_str("&mode=memory");
    Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
}

impl Catalogue {
    /// Inspect without initializing or reconciling SQL. Recreate a missing
    /// lifetime-lock marker so a partially initialized library stays retryable.
    pub(crate) fn probe_setup_preferences(root: &Path) -> Result<Option<Value>> {
        let root = crate::storage::checked_library_root(root)?;
        let database = root.join(".nna/data/catalogue.sqlite3");
        crate::storage::validate_library_path(&database)?;
        if !database.exists() {
            return Ok(None);
        }
        let _lock = LibraryLock::acquire(&root)?;
        ensure!(
            !root.join(crate::app_reset::DELETE_MARKER).exists(),
            "reset_pending"
        );
        let conn = Connection::open_with_flags(
            setup_probe_uri(&database)?,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            let tables: i64 =
                conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
            ensure!(tables == 0, "catalogue.newer_schema");
            return Ok(None);
        }
        ensure!(version == 1, "catalogue.newer_schema");
        conn.prepare("SELECT summary,in_all FROM products LIMIT 0")?;
        conn.prepare("SELECT seq,kind,id,before_json,after_json FROM sync_outbox LIMIT 0")?;
        raw(&conn, "preferences", "app")
    }

    pub fn open_bound(root: &Path, device_dir: &Path) -> Result<Self> {
        let root = crate::storage::checked_library_root(root)?;
        let lock = Arc::new(LibraryLock::acquire(&root)?);
        ensure!(
            !root.join(crate::app_reset::DELETE_MARKER).exists(),
            "reset_pending"
        );
        // Even initialization follows the lock: a rejected opener cannot create
        // schema, reconcile a badge or start a service in the owned library.
        let data = root.join(".nna").join("data");
        crate::storage::validate_library_path(&data.join("catalogue.sqlite3"))?;
        let catalogue = Self::open_at(&data, device_dir, Some(root), Some(lock))?;
        let mut conn = catalogue
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("catalogue.lock"))?;
        if rows(&conn, "root")?.is_empty() {
            let id = Uuid::new_v4().to_string();
            let tx = conn.transaction()?;
            set_raw(
                &tx,
                "root",
                &id,
                &json!({"id":id,"path":".","libraryRelative":true,"status":"online","active":true}),
            )?;
            let mut prefs =
                raw(&tx, "preferences", "app")?.context("catalogue.preferences_missing")?;
            prefs["libraryRootId"] = json!(id);
            set_raw(&tx, "preferences", "app", &prefs)?;
            tx.commit()?;
        }
        drop(conn);
        Ok(catalogue)
    }

    pub fn device_dir(&self) -> PathBuf {
        self.device_dir.clone()
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn library_root(&self) -> Option<&Path> {
        self.library_root.as_deref()
    }
    pub fn check_library_access(&self) -> Result<()> {
        if let Some(lock) = &self.library_lock {
            lock.check_available()?;
        }
        Ok(())
    }
    pub fn check_library_available(&self) -> Result<()> {
        self.check_library_access()
    }
    pub(crate) fn retain_move_lock(
        &self,
        lock: Arc<LibraryLock>,
        permit: CatalogueWorkPermit,
    ) -> Result<()> {
        *self
            .pending_move
            .lock()
            .map_err(|_| anyhow::anyhow!("catalogue.lock"))? = Some((lock, permit));
        Ok(())
    }
    pub(crate) fn release_move_hold(&self) -> Result<()> {
        self.pending_move
            .lock()
            .map_err(|_| anyhow::anyhow!("catalogue.lock"))?
            .take();
        Ok(())
    }
    pub(crate) fn snapshot_database(&self, path: &Path) -> Result<()> {
        self.check_library_access()?;
        ensure!(!path.exists(), "destination_not_empty");
        let conn = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("catalogue.lock"))?;
        conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        let copy = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let valid: String = copy.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        ensure!(valid == "ok", "file_integrity_failed");
        Ok(())
    }

    pub(super) fn display_roots(&self, conn: &Connection) -> Result<Vec<Value>> {
        let mut roots = rows(conn, "root")?;
        if let Some(path) = self.library_root() {
            for root in &mut roots {
                if root["libraryRelative"] == true {
                    let relative = text(root, "path");
                    if relative != "." {
                        portable::validate_relative(relative)?;
                    }
                    root["path"] = json!(if relative == "." {
                        path.to_path_buf()
                    } else {
                        path.join(relative)
                    });
                }
            }
        }
        Ok(roots)
    }

    pub(super) fn resolve_previews(&self, result: &mut Value) -> Result<()> {
        let resolve = |product: &mut Value| -> Result<()> {
            if product["thumbnailHidden"] != true {
                if let Some(relative) = product["thumbnailPath"].as_str() {
                    portable::validate_relative(relative)?;
                    product["previewUrl"] = json!(self.dir.join(relative));
                }
            }
            Ok(())
        };
        if let Some(products) = result.get_mut("products").and_then(Value::as_array_mut) {
            for product in products {
                resolve(product)?;
            }
        }
        if result["id"].is_string() {
            resolve(result)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
