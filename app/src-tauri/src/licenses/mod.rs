//! License filter. VN3, UV License and CC0 facts live in `licenses.sqlite3` beside the
//! catalogue, so turning the feature off keeps them and deleting them leaves the catalogue
//! untouched. Sync reads public BOOTH pages and the documents they link, plus a 0-yen
//! license download with an account that holds the product. A failed read never replaces
//! a result, and a user's edit always wins over a synced one.
mod fetch;
mod parse;
#[cfg(test)]
mod tests;

use crate::booth::BoothService;
use crate::catalogue::Catalogue;
use anyhow::{bail, ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Listener};

pub const JOB_TYPE: &str = "licenses:sync";
/// An asset whose read had started when the App stopped.
const INTERRUPTED: &str = "LICENSE_READ_INTERRUPTED";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS results(product_id TEXT PRIMARY KEY,state TEXT NOT NULL,kind TEXT,version TEXT,source TEXT,answers TEXT,code TEXT,checked_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS edits(product_id TEXT PRIMARY KEY,cells TEXT NOT NULL,edited_at TEXT NOT NULL);
PRAGMA user_version=1;";

/// The value a filter cell can hold. V (credit) is required or not, X (special notes)
/// present or not, every other VN3 item allowed or not.
fn cell_values(letter: &str) -> &'static [&'static str] {
    match letter {
        "V" => &["required", "notRequired"],
        "X" => &["yes", "no"],
        _ => &["allow", "deny"],
    }
}

fn vn3_cell(letter: &str, code: &str) -> Option<&'static str> {
    match (letter, code) {
        ("V", "credit_required") => Some("required"),
        ("V", "credit_optional_welcome" | "credit_not_required") => Some("notRequired"),
        ("X", "none") => Some("no"),
        ("X", "present") => Some("yes"),
        ("V" | "X", _) | (_, "not_applicable") => None,
        // Asking the rights holder first is not permission; private use alone is not use.
        (
            _,
            "ask"
            | "private_only"
            | "deny"
            | "deny_create"
            | "deny_distribute"
            | "deny_except_private",
        ) => Some("deny"),
        (
            _,
            "allow" | "commercial" | "doujin" | "noncommercial" | "allow_public" | "allow_limited"
            | "allow_segregated" | "allow_if_modified" | "free_only" | "free_only_bound"
            | "free_only_users" | "bound" | "users_only",
        ) => Some("allow"),
        _ => None,
    }
}

/// Synced facts as filter cells. The database keeps each source's own answers, so a
/// changed reading needs no new sync.
pub fn cells(kind: &str, answers: &Value) -> BTreeMap<String, String> {
    let mut cells = BTreeMap::new();
    let mut set = |letter: &str, value: &str| {
        cells.insert(letter.to_owned(), value.to_owned());
    };
    match kind {
        "vn3" => {
            for (letter, code) in answers.as_object().into_iter().flatten() {
                if let Some(value) = code.as_str().and_then(|code| vn3_cell(letter, code)) {
                    set(letter, value);
                }
            }
        }
        "uv" => {
            let term = |name: &str| {
                if answers[name] == true {
                    "allow"
                } else {
                    "deny"
                }
            };
            for (letter, value) in [
                ("A", "allow"),
                ("B", term("ccu")),
                ("C", "allow"),
                ("F", term("seu")),
                ("G", term("veu")),
                ("H", term("pru")),
                ("I", "allow"),
                ("J", "allow"),
                ("K", "allow"),
                ("L", "allow"),
                ("M", "deny"),
                ("N", term("ddw")),
                ("S", term("dmd")),
                ("U", term("ddw")),
                ("V", "required"),
                (
                    "X",
                    if answers["remarks"] == true {
                        "yes"
                    } else {
                        "no"
                    },
                ),
            ] {
                set(letter, value);
            }
        }
        "cc0" => {
            for letter in parse::LETTERS.chars().map(String::from) {
                let value = match letter.as_str() {
                    "V" => "notRequired",
                    "X" => "no",
                    _ => "allow",
                };
                set(&letter, value);
            }
        }
        _ => {}
    }
    cells
}

/// Only known cells and values; anything else is dropped.
fn valid_cells(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(letter, v)| {
            let v = v.as_str()?;
            (letter.len() == 1
                && parse::LETTERS.contains(letter.as_str())
                && cell_values(letter).contains(&v))
            .then(|| (letter.clone(), v.to_owned()))
        })
        .collect()
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn booth_item(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https" || !(host == "booth.pm" || host.ends_with(".booth.pm")) {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let at = segments.iter().position(|s| *s == "items")?;
    let id = segments.get(at + 1)?;
    (at <= 1
        && segments.len() == at + 2
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit()))
    .then(|| (*id).to_owned())
}

/// A product whose BOOTH page may hold a license, with an account that holds it.
struct Target {
    product: String,
    item: String,
    account: Option<String>,
}

fn targets(catalogue: &Catalogue) -> Result<Vec<Target>> {
    let rank = |source: &str| match source {
        "purchase" | "library" => 0,
        "gift" => 1,
        "free" => 2,
        _ => 3,
    };
    // A holder signed in on this device can download the 0-yen file; prefer one.
    let mut signed_in: HashMap<String, bool> = HashMap::new();
    let mut usable = |account: &str| {
        *signed_in.entry(account.to_owned()).or_insert_with(|| {
            catalogue
                .get("account", account)
                .ok()
                .flatten()
                .is_some_and(|record| {
                    record["disconnected"] != true
                        && crate::stores::has_local_session(catalogue, &record)
                })
        })
    };
    Ok(catalogue
        .license_sources()?
        .into_iter()
        .filter_map(|(product, facts)| {
            let item = match facts["provider"].as_str() {
                Some("booth")
                    if facts["availability"] != "unavailable"
                        && product.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    product.clone()
                }
                Some("local") => booth_item(facts["sourceUrl"].as_str()?)?,
                _ => return None,
            };
            let account = facts["provenance"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    Some((
                        row["accountId"].as_str()?,
                        rank(row["source"].as_str().unwrap_or_default()),
                    ))
                })
                .filter(|(_, rank)| *rank < 3)
                .min_by_key(|(account, rank)| (!usable(account), *rank))
                .map(|(account, _)| account.to_owned());
            Some(Target {
                product,
                item,
                account,
            })
        })
        .collect())
}

/// A job's failure code; one that is not a known code is the license sync's own.
fn job_error(error: &anyhow::Error) -> String {
    match crate::booth::error_code(error) {
        code if code == "BOOTH_OPERATION_FAILED" => "LICENSE_SYNC_FAILED".into(),
        code => code,
    }
}

#[derive(Clone)]
pub struct LicenseService {
    inner: Arc<Inner>,
}
struct Inner {
    catalogue: Catalogue,
    booth: BoothService,
    path: PathBuf,
    db: Mutex<Option<Connection>>,
    /// The running job's id and cancel flag.
    running: Mutex<Option<(String, Arc<AtomicBool>)>>,
    automatic_pending: AtomicBool,
}

impl LicenseService {
    pub fn new(catalogue: Catalogue, booth: BoothService) -> Self {
        Self {
            inner: Arc::new(Inner {
                path: catalogue.data_dir().join("licenses.sqlite3"),
                catalogue,
                booth,
                db: Mutex::new(None),
                running: Mutex::new(None),
                automatic_pending: AtomicBool::new(false),
            }),
        }
    }

    fn enabled(&self) -> bool {
        self.inner
            .catalogue
            .get("preferences", "app")
            .ok()
            .flatten()
            .is_some_and(|preferences| preferences["licenseFilter"] == true)
    }

    /// Runs `work` on the license database, creating it only when `create`.
    fn with_db<T>(
        &self,
        create: bool,
        work: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<Option<T>> {
        let mut db = self
            .inner
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("licenses.lock"))?;
        if db.is_none() {
            if !create && !self.inner.path.is_file() {
                return Ok(None);
            }
            let conn = Connection::open(&self.inner.path)?;
            conn.busy_timeout(std::time::Duration::from_secs(10))?;
            let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            ensure!(version <= 1, "licenses.newer_schema");
            // Libraries may live on shares, like the catalogue: rollback journal, full sync.
            conn.execute_batch("PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")?;
            conn.execute_batch(SCHEMA)?;
            *db = Some(conn);
        }
        work(db.as_ref().unwrap()).map(Some)
    }

    /// Effective cells of every product that has any: an edit replaces the synced cells.
    fn effective(&self) -> Result<HashMap<String, BTreeMap<String, String>>> {
        Ok(self
            .with_db(false, |conn| {
                let mut map = HashMap::new();
                let mut found = conn
                    .prepare("SELECT product_id,kind,answers FROM results WHERE state='found'")?;
                for row in found.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })? {
                    let (product, kind, answers) = row?;
                    map.insert(
                        product,
                        cells(
                            &kind,
                            &serde_json::from_str(&answers).unwrap_or(Value::Null),
                        ),
                    );
                }
                let mut edits = conn.prepare("SELECT product_id,cells FROM edits")?;
                for row in edits.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })? {
                    let (product, edited) = row?;
                    map.insert(
                        product,
                        valid_cells(&serde_json::from_str(&edited).unwrap_or(Value::Null)),
                    );
                }
                Ok(map)
            })?
            .unwrap_or_default())
    }

    /// Products whose cells hold every chosen value; `None` when nothing is chosen.
    /// A product without license facts never matches a chosen cell.
    pub fn matching(&self, filter: &Value) -> Result<Option<Vec<String>>> {
        let wanted = valid_cells(filter);
        if wanted.is_empty() {
            return Ok(None);
        }
        let mut ids: Vec<String> = self
            .effective()?
            .into_iter()
            .filter(|(_, cells)| {
                wanted
                    .iter()
                    .all(|(letter, value)| cells.get(letter) == Some(value))
            })
            .map(|(product, _)| product)
            .collect();
        ids.sort();
        Ok(Some(ids))
    }

    pub async fn command(&self, app: &AppHandle, action: &str, args: Value) -> Result<Value> {
        match action {
            "status" => {
                let counts = self.with_db(false, |conn| {
                    let count = |sql: &str| conn.query_row(sql, [], |row| row.get::<_, i64>(0));
                    Ok(json!({
                        "found": count("SELECT COUNT(*) FROM results WHERE state='found'")?,
                        "edited": count("SELECT COUNT(*) FROM edits")?,
                    }))
                })?;
                Ok(
                    json!({"exists": counts.is_some(), "counts": counts, "running": self.running_job()}),
                )
            }
            "get" => {
                let product = args["productId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("INVALID_ARGUMENT_productId"))?;
                let detail = self.with_db(false, |conn| {
                    let result = conn
                        .query_row(
                            "SELECT kind,version,source,answers,checked_at FROM results WHERE product_id=?1 AND state='found'",
                            [product],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
                        )
                        .optional()?;
                    let edited: Option<String> = conn.query_row("SELECT cells FROM edits WHERE product_id=?1", [product], |row| row.get(0)).optional()?;
                    Ok(json!({
                        "synced": result.as_ref().map(|(kind, version, source, answers, checked)| json!({
                            "kind": kind, "version": version, "source": source, "checkedAt": checked,
                            "cells": cells(kind, &serde_json::from_str(answers).unwrap_or(Value::Null)),
                        })),
                        "edited": edited.map(|e| valid_cells(&serde_json::from_str(&e).unwrap_or(Value::Null))),
                    }))
                })?;
                Ok(detail.unwrap_or(json!({"synced": null, "edited": null})))
            }
            "set" => {
                let product = args["productId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("INVALID_ARGUMENT_productId"))?
                    .to_owned();
                let edited = (!args["cells"].is_null()).then(|| valid_cells(&args["cells"]));
                self.with_db(edited.is_some(), |conn| {
                    match &edited {
                        Some(cells) => conn.execute(
                            "INSERT INTO edits(product_id,cells,edited_at) VALUES(?1,?2,?3) ON CONFLICT(product_id) DO UPDATE SET cells=excluded.cells,edited_at=excluded.edited_at",
                            params![product, json!(cells).to_string(), now()],
                        )?,
                        None => conn.execute("DELETE FROM edits WHERE product_id=?1", [&product])?,
                    };
                    Ok(())
                })?;
                let _ = app.emit(
                    "nna:changed",
                    json!({"scope":"licenses","productIds":[product]}),
                );
                Ok(json!({}))
            }
            "sync" => {
                let ids = args["productIds"].as_array().map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                });
                let job = self
                    .sync(
                        app,
                        ids.map(Selection::Chosen).unwrap_or(Selection::Unfinished),
                    )
                    .await?;
                Ok(json!({"jobId": job}))
            }
            "retry" => {
                let job_id = args["jobId"].as_str().unwrap_or_default();
                let job = self
                    .inner
                    .catalogue
                    .get("job", job_id)?
                    .filter(|job| job["type"] == JOB_TYPE)
                    .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
                let done: BTreeSet<&str> = job["completedProductIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let failed: BTreeSet<&str> = job["failures"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(id, _)| id.as_str())
                    .collect();
                let ids = job["productIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|id| failed.contains(id) || !done.contains(id))
                    .map(str::to_owned)
                    .collect();
                Ok(json!({"jobId": self.sync(app, Selection::Chosen(ids)).await?}))
            }
            "cancel" => {
                self.cancel(args["jobId"].as_str());
                Ok(json!({}))
            }
            "delete" => {
                self.cancel(None);
                for _ in 0..600 {
                    if self.running_job().is_none() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                ensure!(self.running_job().is_none(), "licenses_busy");
                let mut db = self
                    .inner
                    .db
                    .lock()
                    .map_err(|_| anyhow::anyhow!("licenses.lock"))?;
                *db = None;
                for suffix in ["", "-journal"] {
                    let path = PathBuf::from(format!("{}{suffix}", self.inner.path.display()));
                    if path.is_file() {
                        std::fs::remove_file(&path)
                            .map_err(|_| anyhow::anyhow!("licenses_delete_failed"))?;
                    }
                }
                drop(db);
                let _ = app.emit("nna:changed", json!({"scope":"licenses"}));
                Ok(json!({}))
            }
            _ => bail!("UNKNOWN_ACTION"),
        }
    }

    fn running_job(&self) -> Option<String> {
        self.inner
            .running
            .lock()
            .ok()
            .and_then(|running| running.as_ref().map(|(id, _)| id.clone()))
    }

    /// Stops the running job, or only the named one.
    pub fn cancel(&self, job: Option<&str>) {
        if let Ok(running) = self.inner.running.lock() {
            if let Some((id, flag)) = running.as_ref() {
                if job.is_none_or(|job| job == id) {
                    flag.store(true, Ordering::Release);
                }
            }
        }
    }

    /// Resumes a job the last run left open, and once store syncs settle checks the products
    /// no license read has seen yet.
    pub async fn start(&self, app: &AppHandle) -> Result<()> {
        self.follow_syncs(app);
        for job in self.inner.catalogue.list("job")? {
            if job["type"] == JOB_TYPE
                && matches!(job["state"].as_str(), Some("queued" | "running"))
            {
                // A job that cannot resume is left for the next start; reads still follow syncs.
                let _ = self.resume(app, job);
            }
        }
        Ok(())
    }

    fn follow_syncs(&self, app: &AppHandle) {
        let service = self.clone();
        let handle = app.clone();
        app.listen("nna:changed", move |event| {
            let scope = serde_json::from_str::<Value>(event.payload())
                .ok()
                .and_then(|v| v["scope"].as_str().map(str::to_owned));
            if scope.as_deref() != Some("booth")
                || service.inner.automatic_pending.swap(true, Ordering::AcqRel)
            {
                return;
            }
            let service = service.clone();
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                service
                    .inner
                    .automatic_pending
                    .store(false, Ordering::Release);
                let _ = service.automatic(&handle).await;
            });
        });
    }

    /// Continues a job the last run left open; one that cannot continue is cancelled, so no
    /// job stays running with nothing behind it.
    fn resume(&self, app: &AppHandle, mut job: Value) -> Result<()> {
        if self.enabled() && self.running_job().is_none() && self.resume_work(app, &mut job).is_ok()
        {
            return Ok(());
        }
        job["state"] = json!("cancelled");
        job["errorCode"] = json!("licenses_interrupted");
        self.save_job(app, &job)
    }

    fn resume_work(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        let mut done: BTreeSet<String> = job["completedProductIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        // The asset being read when the App stopped is read once more, since the App may just
        // have closed or updated. Stopped on it twice, as when its file crashes the reader, it
        // is not read again automatically; Retry or Sync licenses reads it.
        if let Some(current) = job["current"]
            .as_str()
            .filter(|id| !done.contains(*id))
            .map(str::to_owned)
        {
            if job["resumedCurrent"].as_str() == Some(current.as_str()) {
                self.record(&current, fetch::Outcome::Failed(INTERRUPTED.into()))?;
                job["failures"][&current] = json!({"productId": current, "errorCode": INTERRUPTED});
                if let Some(completed) = job["completedProductIds"].as_array_mut() {
                    completed.push(json!(current));
                }
                done.insert(current);
            } else {
                job["resumedCurrent"] = json!(current);
            }
        }
        let left: BTreeSet<&str> = job["productIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| !done.contains(*id))
            .collect();
        let mut work: Vec<Target> = targets(&self.inner.catalogue)?
            .into_iter()
            .filter(|t| left.contains(t.product.as_str()))
            .collect();
        // The interrupted asset goes first, so a second stop on it is recognised.
        if let Some(current) = job["resumedCurrent"].as_str() {
            if let Some(at) = work.iter().position(|t| t.product == current) {
                let target = work.remove(at);
                work.insert(0, target);
            }
        }
        self.run(app, job.clone(), work)
    }

    async fn automatic(&self, app: &AppHandle) -> Result<()> {
        if !self.enabled() || self.running_job().is_some() {
            return Ok(());
        }
        // BOOTH's own sync ends with this scope; other stores' events find nothing new to read.
        let store_busy = self.inner.catalogue.list("job")?.iter().any(|job| {
            job["type"].as_str().is_some_and(|t| t.starts_with("booth"))
                && matches!(job["state"].as_str(), Some("queued" | "running"))
        });
        if !store_busy {
            self.sync(app, Selection::New).await?;
        }
        Ok(())
    }

    /// Starts a job; `None` when there is nothing to read.
    async fn sync(&self, app: &AppHandle, selection: Selection) -> Result<Option<String>> {
        ensure!(self.enabled(), "licenses_disabled");
        if self.running_job().is_some() {
            if matches!(selection, Selection::New) {
                return Ok(None);
            }
            bail!("licenses_busy");
        }
        let all = targets(&self.inner.catalogue)?;
        let settled: BTreeSet<String> = match selection {
            Selection::Chosen(_) => BTreeSet::new(),
            Selection::Unfinished => self.settled("state IN('found','none')")?,
            Selection::New => self.settled("1=1")?,
        };
        let work: Vec<Target> = match &selection {
            Selection::Chosen(ids) => {
                let chosen: BTreeSet<&String> = ids.iter().collect();
                all.into_iter()
                    .filter(|t| chosen.contains(&t.product))
                    .collect()
            }
            _ => all
                .into_iter()
                .filter(|t| !settled.contains(&t.product))
                .collect(),
        };
        if work.is_empty() {
            return Ok(None);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let job = json!({
            "id": id, "type": JOB_TYPE, "state": "running", "createdAt": now(), "updatedAt": now(),
            "productIds": work.iter().map(|t| t.product.clone()).collect::<Vec<_>>(),
            "completedProductIds": [], "progress": 0, "failures": {},
            "automatic": matches!(selection, Selection::New),
        });
        self.run(app, job, work)?;
        Ok(Some(id))
    }

    fn settled(&self, condition: &str) -> Result<BTreeSet<String>> {
        Ok(self
            .with_db(false, |conn| {
                let mut statement =
                    conn.prepare(&format!("SELECT product_id FROM results WHERE {condition}"))?;
                let ids = statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<BTreeSet<_>>>()?;
                Ok(ids)
            })?
            .unwrap_or_default())
    }

    fn run(&self, app: &AppHandle, mut job: Value, work: Vec<Target>) -> Result<()> {
        let permit = self.inner.catalogue.begin_catalogue_work()?;
        let cancel = Arc::new(AtomicBool::new(false));
        let job_id = job["id"].as_str().unwrap_or_default().to_owned();
        {
            let mut running = self
                .inner
                .running
                .lock()
                .map_err(|_| anyhow::anyhow!("licenses.lock"))?;
            ensure!(running.is_none(), "licenses_busy");
            *running = Some((job_id.clone(), cancel.clone()));
        }
        job["state"] = json!("running");
        if let Err(error) = self.save_job(app, &job) {
            if let Ok(mut running) = self.inner.running.lock() {
                *running = None;
            }
            return Err(error);
        }
        let service = self.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let outcome = service.work(&app, &mut job, work, &cancel).await;
            if let Some(job) = job.as_object_mut() {
                job.remove("current");
            }
            job["state"] = json!(match &outcome {
                Err(_) => "failed",
                Ok(()) if cancel.load(Ordering::Acquire) => "cancelled",
                Ok(()) if job["failures"].as_object().is_some_and(|f| !f.is_empty()) => "failed",
                Ok(()) => "completed",
            });
            if let Err(error) = &outcome {
                job["errorCode"] = json!(job_error(error));
            }
            job["completedAt"] = json!(now());
            let _ = service.save_job(&app, &job);
            if let Ok(mut running) = service.inner.running.lock() {
                *running = None;
            }
            let _ = app.emit("nna:changed", json!({"scope":"licenses"}));
        });
        Ok(())
    }

    async fn work(
        &self,
        app: &AppHandle,
        job: &mut Value,
        work: Vec<Target>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let reader = fetch::Reader::new(self.inner.booth.clone())?;
        let total = job["productIds"]
            .as_array()
            .map_or(work.len(), Vec::len)
            .max(1);
        let mut saved = std::time::Instant::now();
        let mut changed = vec![];
        for target in work {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            // Say which asset is being read before reading it, so a file that brings the App
            // down is not read again at the next start or by the next automatic read.
            job["current"] = json!(target.product);
            job["updatedAt"] = json!(now());
            self.inner
                .catalogue
                .put("job", job["id"].as_str().unwrap_or_default(), job.clone())?;
            self.with_db(true, |conn| {
                conn.execute(
                    "INSERT INTO results(product_id,state,code,checked_at) VALUES(?1,'failed',?2,?3) ON CONFLICT(product_id) DO NOTHING",
                    params![target.product, INTERRUPTED, now()],
                )?;
                Ok(())
            })?;
            let holder = target.account.as_deref().map(|account| Holder {
                booth: &self.inner.booth,
                app,
                account,
            });
            let outcome = reader
                .read(
                    &target.item,
                    holder.as_ref().map(|h| h as &dyn fetch::FreeDownload),
                )
                .await;
            if let fetch::Outcome::Failed(code) = &outcome {
                job["failures"][&target.product] =
                    json!({"productId": target.product, "errorCode": code});
            } else if let Some(failures) = job["failures"].as_object_mut() {
                failures.remove(&target.product);
            }
            self.record(&target.product, outcome)?;
            changed.push(target.product.clone());
            if let Some(done) = job["completedProductIds"].as_array_mut() {
                done.push(json!(target.product));
            }
            let done = job["completedProductIds"].as_array().map_or(0, Vec::len);
            job["progress"] = json!((100 * done / total).min(100));
            if saved.elapsed() > std::time::Duration::from_secs(2) {
                self.save_job(app, job)?;
                let _ = app.emit(
                    "nna:changed",
                    json!({"scope":"licenses","productIds":changed}),
                );
                changed = vec![];
                saved = std::time::Instant::now();
            }
        }
        let _ = app.emit(
            "nna:changed",
            json!({"scope":"licenses","productIds":changed}),
        );
        Ok(())
    }

    /// A found license replaces the last one; nothing else ever replaces a found one.
    fn record(&self, product: &str, outcome: fetch::Outcome) -> Result<()> {
        self.with_db(true, |conn| {
            match outcome {
                fetch::Outcome::Found(found) => conn.execute(
                    "INSERT INTO results(product_id,state,kind,version,source,answers,code,checked_at) VALUES(?1,'found',?2,?3,?4,?5,NULL,?6) ON CONFLICT(product_id) DO UPDATE SET state='found',kind=excluded.kind,version=excluded.version,source=excluded.source,answers=excluded.answers,code=NULL,checked_at=excluded.checked_at",
                    params![product, found.kind, found.version, found.source, found.answers.to_string(), now()],
                )?,
                fetch::Outcome::Missing => conn.execute(
                    "INSERT INTO results(product_id,state,checked_at) VALUES(?1,'none',?2) ON CONFLICT(product_id) DO UPDATE SET state='none',checked_at=excluded.checked_at WHERE state<>'found'",
                    params![product, now()],
                )?,
                fetch::Outcome::Failed(code) => conn.execute(
                    "INSERT INTO results(product_id,state,code,checked_at) VALUES(?1,'failed',?2,?3) ON CONFLICT(product_id) DO UPDATE SET state='failed',code=excluded.code,checked_at=excluded.checked_at WHERE state<>'found'",
                    params![product, code, now()],
                )?,
            };
            Ok(())
        })?;
        Ok(())
    }

    fn save_job(&self, app: &AppHandle, job: &Value) -> Result<()> {
        let mut job = job.clone();
        job["updatedAt"] = json!(now());
        let id = job["id"].as_str().unwrap_or_default().to_owned();
        self.inner.catalogue.put("job", &id, job)?;
        let _ = app.emit("nna:changed", json!({"scope":"jobs"}));
        Ok(())
    }
}

/// An account that holds the product, signed in through the App.
struct Holder<'a> {
    booth: &'a BoothService,
    app: &'a AppHandle,
    account: &'a str,
}
impl fetch::FreeDownload for Holder<'_> {
    fn download<'a>(
        &'a self,
        route: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(
            self.booth
                .free_license_file(self.app, self.account, route, fetch::DOCUMENT_LIMIT),
        )
    }
}

enum Selection {
    /// Products the user chose, read again whatever their last result.
    Chosen(Vec<String>),
    /// Everything not yet found or proven to have no license.
    Unfinished,
    /// Products no license read has seen.
    New,
}
