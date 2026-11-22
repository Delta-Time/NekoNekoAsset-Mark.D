#[cfg(test)]
mod targeted_receipt_tests;

mod acquisition_diagnostics;
mod checkpoints;
mod free_history;
mod free_import;
mod publication;
mod records;
mod routes;
use super::{
    field, free_item, http,
    incremental::{Baseline, Tracker, UpdatedRead, UpdatedTracker},
    metadata, now, parser, safety, BoothService,
};
pub(super) use acquisition_diagnostics::FreeItemDiagnostic;
use acquisition_diagnostics::{
    select_acquisition_diagnostic_detail, select_free_item_diagnostic_route,
    select_targeted_receipt_proof, DiagnosticReservation,
};
use anyhow::{bail, ensure, Result};
use checkpoints::{
    checkpoint_finished_after_collection, record_incomplete, resumable_checkpoint,
    source_has_incomplete_product, Checkpoint, SourceScan,
};
use free_history::{
    augment_free_row, eligible_free_rows, free_history_order, free_history_positions,
    remember_free_history, saved_free_history, settled_free_absence, stage_free_orders,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use publication::{
    additive_version, observed_file_ids, observed_grant_rows, product_draft_scope,
    product_publication_done, publication_entries, publication_entry_has_complete_grant,
    publication_entry_published, publication_manifest, publication_membership_proven,
    publication_products, publication_progress, published_product_count, ready_product_entries,
    remember_product_context, set_collecting_totals, source_incomplete_product_count,
    EarlyPublications, ProductPublication, Publications,
};
#[cfg(test)]
pub(crate) use records::assert_declared;
use records::{Acquisition, Entitlement, RemoteFile};
use routes::{
    available_routes, detail_route, limit_listing_pages, overlap_changed, read_every_page,
    selected_purchase_routes,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
const FREE_ITEM_ADAPTER_VERSION: u32 = 3;
pub(super) fn source_changed(app: &AppHandle, ids: impl IntoIterator<Item = String>) {
    let product_ids: BTreeSet<_> = ids.into_iter().collect();
    if !product_ids.is_empty() {
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change::products("catalogue", product_ids),
        );
    }
}

#[derive(Clone)]
struct PublicPrefetch {
    requests: mpsc::UnboundedSender<PublicRequest>,
    enqueued: std::sync::Arc<Mutex<BTreeSet<String>>>,
    early: std::sync::Arc<Mutex<EarlyPublications>>,
    support_pages: Arc<Mutex<SupportPages>>,
}
#[derive(Default)]
struct SupportPages {
    observations: BTreeMap<String, Value>,
    reused: usize,
}
impl SupportPages {
    fn remember(&mut self, evidence: &free_item::Evidence, body: &str) -> Result<()> {
        // The free adapter has already checked same-item canonical/OG/root identity,
        // login/challenge and file controls. Retain only its public support facts.
        if !evidence.manifest_complete
            && evidence.error_code.as_deref() != Some("FREE_OFFER_UNAVAILABLE")
        {
            return Ok(());
        }
        let mut observation = json!({});
        metadata::apply_public_support_html(&mut observation, body)?;
        if observation["publicSupportObservedAt"].is_string() {
            self.observations
                .insert(evidence.product_id.clone(), observation);
        }
        Ok(())
    }
    fn apply(&mut self, id: &str, product: &mut Value) -> bool {
        let Some(observation) = self.observations.get(id) else {
            return false;
        };
        // The observation holds only what the support section wrote.
        for (key, value) in observation.as_object().into_iter().flatten() {
            product[key] = value.clone();
        }
        self.reused += 1;
        true
    }
}
/// A product's public request that fails transiently gets one later attempt
/// after the queue drains; a repeated failure ends that round.
#[derive(Default)]
struct PublicRetry {
    deferred: std::collections::VecDeque<(PublicRequest, String)>,
    retrying: bool,
}
impl PublicRetry {
    /// Observations to settle now; a first transient failure is held back.
    fn settle(
        &mut self,
        request: PublicRequest,
        result: Result<Value, String>,
    ) -> Vec<(String, Result<Value, String>)> {
        let code = match result {
            Err(code)
                if matches!(
                    code.as_str(),
                    "NETWORK_UNAVAILABLE" | "PUBLIC_METADATA_FAILED"
                ) =>
            {
                code
            }
            result => return vec![(request.product, result)],
        };
        if !self.retrying {
            self.deferred.push_back((request, code));
            return vec![];
        }
        // Still failing after the pause: settle the rest without more requests.
        std::iter::once((request.product, Err(code)))
            .chain(
                self.deferred
                    .drain(..)
                    .map(|(request, code)| (request.product, Err(code))),
            )
            .collect()
    }
    fn next(&mut self) -> Option<PublicRequest> {
        self.retrying = true;
        self.deferred.pop_front().map(|(request, _)| request)
    }
}
/// Incremental syncs read public metadata only for products without a settled
/// observation. A canonical 404/410 settles removal until a Full sync rereads it.
fn public_refresh_needed(product: Option<&Value>) -> bool {
    product.is_none_or(|p| {
        p["publicMetadataObservedAt"].is_null() && p["publicPageAvailability"] != "unavailable"
            || p["publicMetadataState"] == "incomplete"
    })
}
impl PublicPrefetch {
    fn enqueue_once(&self, product: &str) -> Result<bool> {
        let mut enqueued = self
            .enqueued
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        if enqueued.contains(product) {
            return Ok(false);
        }
        if enqueued.len() >= 10000 {
            bail!("SOURCE_TRAVERSAL_LIMIT");
        }
        enqueued.insert(product.into());
        Ok(true)
    }
}
struct PublicRequest {
    product: String,
    draft: Value,
    account: String,
    source: String,
    run_id: String,
}
impl PublicRequest {
    fn draft_id(&self) -> String {
        format!("booth-product:{}:{}", self.run_id, self.product)
    }
    fn saved_product(&self, saved: &Value) -> Option<Value> {
        (saved["accountId"] == self.account
            && saved["source"] == self.source
            && saved["runId"] == self.run_id
            && saved["productId"] == self.product
            && saved["enriched"] == true
            && saved["product"]["id"] == self.product)
            .then(|| saved["product"].clone())
    }
}
fn trace_source_closure(checkpoint: &Checkpoint, publications: &Publications) {
    let Some(&(unbound, head_stable)) = publications
        .source_closures
        .get(&(checkpoint.account_id.clone(), checkpoint.source.clone()))
    else {
        return;
    };
    crate::support_trace::source_closure(
        &checkpoint.source,
        checkpoint.terminal,
        checkpoint.pending.len() as u64,
        checkpoint.unresolved as u64,
        checkpoint.enrichment_pending.len() as u64,
        source_incomplete_product_count(checkpoint, &publications.incomplete_scopes),
        unbound,
        head_stable,
    );
}
fn trace_source_assembly(
    checkpoint: &Checkpoint,
    publications: &Publications,
    terminal_unproven_observed: bool,
) {
    let incomplete_products =
        source_incomplete_product_count(checkpoint, &publications.incomplete_scopes);
    if incomplete_products != 0 {
        crate::support_trace::source_reason(
            &checkpoint.source,
            "SOURCE_MANIFEST_INCOMPLETE",
            incomplete_products,
        );
    }
    if !checkpoint.terminal && !terminal_unproven_observed {
        crate::support_trace::source_reason(&checkpoint.source, "SOURCE_TERMINAL_UNPROVEN", 1);
    }
    trace_source_closure(checkpoint, publications);
}
fn trace_checked_journal(page: &parser::Page, unbound: bool) -> bool {
    // Journal observations can repeat within an attempt; these are neither
    // unique orders nor committed catalogue counts. Only closed codes escape.
    for code in page
        .rows
        .iter()
        .filter_map(|row| row.error_code.as_deref())
        .chain(
            page.free_item
                .as_ref()
                .and_then(|e| e.error_code.as_deref()),
        )
    {
        crate::support_trace::source_reason(&page.source, crate::support::safe_code(code), 1);
    }
    let terminal_unproven = !page.is_detail && !page.terminal && page.next.is_empty();
    if terminal_unproven {
        crate::support_trace::source_reason(&page.source, "SOURCE_TERMINAL_UNPROVEN", 1);
    }
    if unbound {
        crate::support_trace::source_reason(&page.source, "SOURCE_MEMBERSHIP_UNBOUND", 1);
    }
    terminal_unproven
}
fn record_source_head(checkpoint: &mut Checkpoint, final_head: &parser::Page) {
    if final_head.fingerprint != checkpoint.head {
        checkpoint.unresolved += 1;
        crate::support_trace::source_reason(&checkpoint.source, "SOURCE_HEAD_CHANGED", 1);
    }
}
fn source_local_failure(code: &str) -> bool {
    matches!(
        code,
        "COLLECTOR_UNSUPPORTED"
            | "SOURCE_UNSUPPORTED"
            | "CHECKPOINT_INVALID"
            | "NO_RECOGNIZED_ROWS"
            | "SOURCE_MARKER_UNKNOWN"
            | "DIGITAL_PROOF_UNPROVEN"
            | "SOURCE_TRAVERSAL_LIMIT"
            | "METADATA_TOO_LARGE"
    )
}
/// The store answered definitively about one asset: it refused or moved its page, or sent
/// one that cannot be read. That asset stays as it was and the job goes on. An outage, a
/// server error, throttling or a challenge still stops the job.
fn product_local_failure(code: &str) -> bool {
    matches!(
        code,
        "AUTH_REQUIRED"
            | "PERMISSION_DENIED"
            | "RESPONSE_ROUTE_CHANGED"
            | "UNSAFE_REDIRECT"
            | "METADATA_TOO_LARGE"
            | "METADATA_ENCODING"
            | "PUBLIC_METADATA_INVALID"
            | "PUBLIC_ID_MISMATCH"
    )
}
/// BOOTH refused or moved its recently-updated order, or sent a page that cannot be read.
/// Only that order was at stake, so the App reads every page instead. Signing out, throttling
/// or an outage would stop those pages too, so they stop the job as on any list.
fn refused_updated_order(route: &str, error: &anyhow::Error) -> bool {
    safety::updated_order(route)
        && matches!(
            super::code(error).as_str(),
            "SOURCE_HTTP_FAILED"
                | "PERMISSION_DENIED"
                | "RESPONSE_ROUTE_CHANGED"
                | "UNSAFE_REDIRECT"
                | "METADATA_TOO_LARGE"
                | "METADATA_ENCODING"
                | "SOURCE_MARKER_UNKNOWN"
                | "NO_RECOGNIZED_ROWS"
                | "DIGITAL_PROOF_UNPROVEN"
        )
}
/// A list that reads empty after a complete pass found something may be a passing page, so
/// it certifies nothing and replaces nothing, and the next sync reads the list afresh.
fn contradicts_last_pass(page: &parser::Page, baseline: Option<&Baseline>) -> bool {
    !page.is_detail
        && page.rows.is_empty()
        // An order page lists receipts to read instead of rows.
        && page.details.is_empty()
        && page.terminal
        && page.unresolved == 0
        && baseline.is_some_and(Baseline::found_something)
}
/// A public page the store refused is about that product; a server error may pass.
fn public_status_code(status: reqwest::StatusCode) -> &'static str {
    if status.is_client_error() {
        "PUBLIC_METADATA_INVALID"
    } else {
        "PUBLIC_METADATA_FAILED"
    }
}
/// A receipt or free item page that could not be read: unresolved and without rows, so its
/// asset stays as it was and the source keeps its membership uncertified.
fn unreadable_page(source: &str, route: &str, code: &str) -> Result<parser::Page> {
    if let Some(id) = safety::free_item_id(route).filter(|_| source == "free") {
        return free_item::Evidence::unknown(id, code, false).page();
    }
    crate::support_trace::source_reason(source, code, 1);
    Ok(parser::Page {
        source: source.into(),
        route: route.into(),
        rows: vec![],
        next: vec![],
        details: vec![],
        terminal: false,
        unresolved: 1,
        fingerprint: safety::hash(format!("unreadable:{route}:{code}")),
        is_detail: true,
        ordering: parser::Ordering::Unknown,
        free_item: None,
    })
}
/// The checkpoint a selected refresh of one account's source publishes through.
fn selected_checkpoint_key(job: &Value, account: &str, source: &str) -> Result<String> {
    Ok(format!(
        "booth-selected:{}:{account}:{source}",
        field(job, "id")?
    ))
}
/// Rows the parser could not tie to an asset. A page with no such row counts its gaps.
fn unbound_row_count(page: &parser::Page) -> usize {
    let rows = page
        .rows
        .iter()
        .filter(|row| {
            row.product_id.is_none()
                && row
                    .error_code
                    .as_deref()
                    .is_some_and(|code| code != "BOOTH_WITHHELD_PRODUCT_ID")
        })
        .count();
    if rows == 0 {
        page.unresolved
    } else {
        rows
    }
}
/// Whether what this scan left open still lets the job complete. A listing whose end or pages
/// could not be proven does not, nor does one where no row binds to an asset: the parser no
/// longer understands that page. Every other gap is an asset or row the store answered for,
/// or a listing that moved while it was read (a purchase arrived, a seller edited a file):
/// what was read is published, membership stays uncertified and the next sync rereads it.
fn source_gaps_local(
    checkpoint: &Checkpoint,
    final_head: &str,
    terminal_unproven: bool,
    assets_left: bool,
    rows: usize,
) -> bool {
    let read_to_end =
        checkpoint.terminal && checkpoint.pending.is_empty() && !checkpoint.head.is_empty();
    read_to_end
        && !terminal_unproven
        && (checkpoint.head != final_head || assets_left || rows > 0)
        && !(rows > 0 && checkpoint.product_ids.is_empty())
}

/// What a BOOTH source scan reads with; fixed while the scan runs.
#[derive(Clone, Copy)]
struct SourceRead<'a> {
    app: &'a AppHandle,
    account: &'a str,
    source: &'a str,
    mode: &'a str,
    client: &'a reqwest::Client,
    selected: Option<&'a BTreeSet<String>>,
    prefetch: Option<&'a PublicPrefetch>,
    key: &'a str,
}

/// The rows one source pass read, in page order, and what its pages left unproven.
struct PassRows {
    current_rows: Vec<parser::Row>,
    updated_rows: Vec<parser::Row>,
    free_evidence: BTreeMap<String, super::free_item::Evidence>,
    row_routes: BTreeMap<String, String>,
    unbound_membership: bool,
    terminal_unproven_observed: bool,
    incomplete_products: BTreeSet<String>,
    availability_products: BTreeSet<String>,
    left_products: BTreeSet<String>,
    unresolved_rows: usize,
}

/// What a source pass settled before assembling its listed products; each one reads it.
#[derive(Clone, Copy)]
struct PassProducts<'a> {
    head_route: &'a str,
    row_routes: &'a BTreeMap<String, String>,
    free_evidence: &'a BTreeMap<String, super::free_item::Evidence>,
    incomplete_products: &'a BTreeSet<String>,
    availability_products: &'a BTreeSet<String>,
    refreshed_products: &'a BTreeSet<String>,
    retained_observations: bool,
    closed_traversal: bool,
    unbound_membership: bool,
    metadata_total: usize,
}

/// What a source pass hands over once its products are assembled, to record and close it.
struct PassClose {
    checkpoint: Checkpoint,
    all_rows: Vec<parser::Row>,
    updated_rows: Vec<parser::Row>,
    records: Vec<(String, String, Value)>,
    baseline_key: String,
    coverage_id: String,
    left_products: BTreeSet<String>,
    incomplete_products: BTreeSet<String>,
    availability_products: BTreeSet<String>,
    enrichment_error: Option<String>,
    retained_observations: bool,
    unbound_membership: bool,
    closed_traversal: bool,
    terminal_unproven_observed: bool,
    unresolved_rows: usize,
}

impl BoothService {
    pub(super) async fn diagnose_product_line_fields(
        &self,
        app: &AppHandle,
        account: &str,
        receipt_id: &str,
    ) -> Result<parser::ProductLineFieldDiagnostics> {
        if !safety::numeric(receipt_id) {
            bail!("INVALID_DIAGNOSTIC_ARGUMENTS");
        }
        let _reservation = DiagnosticReservation::acquire(&self.inner.running, account)?;
        let (body, saved) = self.targeted_receipt_page(app, account, receipt_id).await?;
        let witnessed = saved.is_some();
        let proof = match saved {
            Some(rows) => rows,
            None => parser::parse(&body, "purchase", &format!("/orders/{receipt_id}"))?.rows,
        };
        parser::diagnose_product_line_fields(&body, &proof, receipt_id, witnessed)
    }
    async fn targeted_receipt_page(
        &self,
        app: &AppHandle,
        account: &str,
        receipt_id: &str,
    ) -> Result<(String, Option<Vec<parser::Row>>)> {
        if !safety::numeric(receipt_id) {
            bail!("INVALID_DIAGNOSTIC_ARGUMENTS");
        }
        // An explicitly requested, unsaved receipt is still fetchable. Existing
        // checkpoints, when present, must validate in full before the GET.
        let saved = match self
            .inner
            .catalogue
            .get("checkpoint", &format!("booth:{account}:purchase"))?
        {
            Some(_) => {
                let (checkpoint, journals) = self.acquisition_diagnostic_journals(account)?;
                select_targeted_receipt_proof(&checkpoint, &journals, account, receipt_id)?
            }
            None => None,
        };
        let route = format!("/orders/{receipt_id}");
        let client = self.client(app, account, false).await?;
        let (status, body) = self
            .inner
            .http
            .get(&client, safety::route(&route, "purchase")?)
            .await?;
        if !status.is_success() {
            bail!("SOURCE_HTTP_FAILED");
        }
        Ok((body, saved))
    }
    pub(super) async fn diagnose_receipt_template(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<parser::ReceiptTemplateDiagnostics> {
        let _reservation = DiagnosticReservation::acquire(&self.inner.running, account)?;
        let (body, proof, receipt_id) = self.acquisition_diagnostic_page(app, account).await?;
        parser::diagnose_receipt_template(&body, &proof, &receipt_id)
    }
    pub(super) async fn diagnose_receipt_notice_envelope(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<parser::ReceiptNoticeDiagnostics> {
        let _reservation = DiagnosticReservation::acquire(&self.inner.running, account)?;
        let (body, proof, receipt_id) = self.acquisition_diagnostic_page(app, account).await?;
        parser::diagnose_receipt_notice_envelope(&body, &proof, &receipt_id)
    }
    pub(super) async fn diagnose_receipt_contact_envelope(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<parser::ReceiptContactDiagnostics> {
        let _reservation = DiagnosticReservation::acquire(&self.inner.running, account)?;
        let (body, proof, receipt_id) = self.acquisition_diagnostic_page(app, account).await?;
        parser::diagnose_receipt_contact_envelope(&body, &proof, &receipt_id)
    }
    fn acquisition_diagnostic_journals(&self, account: &str) -> Result<(Checkpoint, Vec<Value>)> {
        let checkpoint: Checkpoint = serde_json::from_value(
            self.inner
                .catalogue
                .get("checkpoint", &format!("booth:{account}:purchase"))?
                .ok_or_else(|| anyhow::anyhow!("DIAGNOSTIC_ENTRY_NOT_FOUND"))?,
        )
        .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let mut journals = Vec::new();
        for id in &checkpoint.journal_ids {
            journals.push(
                self.inner
                    .catalogue
                    .get("checkpoint", id)?
                    .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?,
            );
        }
        Ok((checkpoint, journals))
    }
    async fn acquisition_diagnostic_page(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<(String, Vec<parser::Row>, String)> {
        let (checkpoint, journals) = self.acquisition_diagnostic_journals(account)?;
        let (route, proof) = select_acquisition_diagnostic_detail(&checkpoint, &journals, account)?;
        let client = self.client(app, account, false).await?;
        let (status, body) = self
            .inner
            .http
            .get(&client, safety::route(&route, "purchase")?)
            .await?;
        if !status.is_success() {
            bail!("SOURCE_HTTP_FAILED");
        }
        let receipt_id = safety::route(&route, "purchase")?
            .path()
            .strip_prefix("/orders/")
            .filter(|id| safety::numeric(id))
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?
            .to_owned();
        Ok((body, proof, receipt_id))
    }
    pub(super) async fn diagnose_free_item_entry(
        &self,
        app: &AppHandle,
        account: &str,
        issue: FreeItemDiagnostic,
    ) -> Result<free_item::EntryDiagnostics> {
        let (id, body) = self.free_item_diagnostic_page(app, account, issue).await?;
        free_item::diagnose_entry(&body, &id)
    }
    pub(super) async fn diagnose_free_item_identity(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<free_item::IdentityDiagnostics> {
        let (id, body) = self
            .free_item_diagnostic_page(app, account, FreeItemDiagnostic::Identity)
            .await?;
        free_item::diagnose_identity(&body, &id)
    }
    async fn free_item_diagnostic_page(
        &self,
        app: &AppHandle,
        account: &str,
        issue: FreeItemDiagnostic,
    ) -> Result<(String, String)> {
        let _reservation = DiagnosticReservation::acquire(&self.inner.running, account)?;
        let key = format!("booth:{account}:free");
        let value = self
            .inner
            .catalogue
            .get("checkpoint", &key)?
            .ok_or_else(|| anyhow::anyhow!("DIAGNOSTIC_ENTRY_NOT_FOUND"))?;
        let checkpoint: Checkpoint =
            serde_json::from_value(value).map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let mut journals = vec![];
        for id in &checkpoint.journal_ids {
            journals.push(
                self.inner
                    .catalogue
                    .get("checkpoint", id)?
                    .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?,
            );
        }
        let route = select_free_item_diagnostic_route(&checkpoint, &journals, account, issue)?;
        let id =
            safety::free_item_id(&route).ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let client = self.client(app, account, false).await?;
        let (status, body) = self
            .inner
            .http
            .get(
                &client,
                url::Url::parse(&route).map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?,
            )
            .await?;
        if !status.is_success() {
            bail!("SOURCE_HTTP_FAILED");
        }
        Ok((id.to_owned(), body))
    }
    pub(super) async fn sync_job(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        let args = job["args"].clone();
        let account = field(&args, "accountId")?;
        let sources = if args["source"].as_str().unwrap_or("all") == "all" {
            vec!["library", "purchase", "gift", "free"]
        } else {
            vec![safety::source_name(field(&args, "source")?)?]
        };
        let client = self.client(app, account, false).await?;
        let mode = args["mode"].as_str().unwrap_or("incremental");
        let mut incomplete = false;
        let (mut left_products, mut left_rows) = (BTreeSet::new(), 0);
        let public =
            http::build_client(std::sync::Arc::new(reqwest::cookie::Jar::default()), false)?;
        // Queue unique observations without coupling traversal to public-page latency.
        // Active requests remain bounded to two and source traversal to 10,000 products.
        let (requests_tx, requests) = mpsc::unbounded_channel();
        let early = std::sync::Arc::new(Mutex::new(EarlyPublications {
            remaining_sources: sources.iter().map(|s| s.to_string()).collect(),
            ..EarlyPublications::default()
        }));
        let prefetch = PublicPrefetch {
            requests: requests_tx,
            enqueued: std::sync::Arc::new(Mutex::new(BTreeSet::new())),
            early: early.clone(),
            support_pages: Arc::default(),
        };
        let support_pages = prefetch.support_pages.clone();
        let (observations, results) = mpsc::unbounded_channel();
        let template = job.clone();
        let collect_and_publish = async {
            let mut scans = FuturesUnordered::new();
            for source in sources {
                let mut source_job = template.clone();
                let prefetch = prefetch.clone();
                let client = &client;
                let public = &public;
                scans.push(async move {
                    let mut enriched = BTreeMap::new();
                    let mut publications = Publications::default();
                    let result = self
                        .scan_source(
                            app,
                            &mut source_job,
                            account,
                            source,
                            mode,
                            client,
                            None,
                            public,
                            &mut enriched,
                            &mut publications,
                            Some(&prefetch),
                        )
                        .await;
                    (source, result, publications)
                });
            }
            let mut publications = Publications::default();
            while let Some((source, result, collected)) = scans.next().await {
                match result {
                    Ok(scan) => {
                        incomplete |= scan.incomplete();
                        left_products.extend(scan.products);
                        left_rows += scan.rows;
                    }
                    Err(error) => {
                        let coverage_id = format!("{account}:{source}");
                        let mut coverage = self
                            .inner
                            .catalogue
                            .get("source_coverage", &coverage_id)?
                            .unwrap_or_else(
                                || json!({"id":coverage_id,"accountId":account,"source":source}),
                            );
                        coverage["status"] = json!("incomplete");
                        coverage["coverageComplete"] = json!(false);
                        coverage["errorCode"] = json!(super::code(&error));
                        coverage["updatedAt"] = json!(now());
                        self.inner
                            .catalogue
                            .put("source_coverage", &coverage_id, coverage)?;
                        // Only sign-in evidence needs the human. A 403 can be a passing block or
                        // limit; the job still ends needing action, but startup sync keeps the account.
                        if matches!(
                            super::code(&error).as_str(),
                            "AUTH_REQUIRED" | "AUTH_CHALLENGE"
                        ) {
                            let mut account_record = self.account(account)?;
                            account_record["state"] = json!("needsAction");
                            account_record["status"] = json!("needsAction");
                            account_record["errorCode"] = json!(super::code(&error));
                            self.inner
                                .catalogue
                                .put("account", account, account_record)?;
                        }
                        if !source_local_failure(&super::code(&error)) {
                            return Err(error);
                        }
                        incomplete = true;
                        publications
                            .unbound_sources
                            .insert((account.into(), source.into()));
                        let mut early = early
                            .lock()
                            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
                        early.remaining_sources.remove(source);
                        early
                            .publications
                            .unbound_sources
                            .insert((account.into(), source.into()));
                    }
                }
                for (product, entries) in collected.products {
                    publications
                        .products
                        .entry(product)
                        .or_default()
                        .extend(entries);
                }
                for (product, entries) in collected.availability {
                    publications
                        .availability
                        .entry(product)
                        .or_default()
                        .extend(entries);
                }
                publications.checkpoints.extend(collected.checkpoints);
                publications
                    .source_closures
                    .extend(collected.source_closures);
                publications.final_records.extend(collected.final_records);
                publications.free_orders.extend(collected.free_orders);
                publications
                    .incomplete_scopes
                    .extend(collected.incomplete_scopes);
                publications
                    .unbound_sources
                    .extend(collected.unbound_sources);
            }
            drop(prefetch);
            // Preserve early commits when final source snapshots are merged.
            {
                let mut early = early
                    .lock()
                    .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
                early.closed = true;
                publications
                    .additive_versions
                    .extend(std::mem::take(&mut early.publications.additive_versions));
                for product in &early.published {
                    if let Some(entries) = publications.products.get_mut(product) {
                        for entry in entries {
                            if let Some(Ok(patch)) = early.observations.get(product) {
                                entry
                                    .draft
                                    .as_object_mut()
                                    .unwrap()
                                    .extend(patch.as_object().unwrap().clone());
                                entry.needs_enrichment = false;
                            }
                        }
                    }
                }
            }
            self.publish_pipeline(app, job, &mut publications, results)
                .await
        };
        let (refused, ()) = tokio::try_join!(
            collect_and_publish,
            self.prefetch_metadata(
                app,
                &template,
                &early,
                &public,
                requests,
                observations,
                &support_pages
            )
        )?;
        job["progress"]["publicHtmlReused"] = json!(
            support_pages
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                .reused
        );
        left_products.extend(refused);
        record_incomplete(job, &left_products, left_rows);
        if incomplete {
            bail!("SYNC_INCOMPLETE");
        }
        Ok(())
    }
    pub(super) async fn update_job(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        if job["args"]["freeImport"] == true {
            return self.free_import_job(app, job).await;
        }
        let ids: BTreeSet<String> = job["args"]["productIds"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let mut owned_sources = self.inner.catalogue.list("entitlement")?;
        owned_sources.extend(
            self.inner
                .catalogue
                .list("acquisition")?
                .into_iter()
                .filter(|record| record["source"] == "free"),
        );
        let accounts: BTreeSet<String> = owned_sources
            .iter()
            .filter(|e| e["productId"].as_str().is_some_and(|p| ids.contains(p)))
            .filter(|e| {
                job["args"]["publicOnly"] != true
                    && job["accountId"]
                        .as_str()
                        .is_none_or(|account| e["accountId"] == account)
            })
            .filter_map(|e| e["accountId"].as_str().map(str::to_owned))
            .collect();
        let owned_accounts_present = !accounts.is_empty();
        let accounts: BTreeSet<_> = accounts
            .into_iter()
            .filter(|account| {
                self.known_account(account).ok().is_some_and(|record| {
                    crate::stores::has_local_session(&self.inner.catalogue, &record)
                })
            })
            .collect();
        if owned_accounts_present && accounts.is_empty() {
            anyhow::bail!("AUTH_REQUIRED");
        }
        let mut incomplete = false;
        let (mut left_products, mut left_rows) = (BTreeSet::new(), 0);
        // Purchases whose receipts could not settle them: counted, public page still read.
        let mut kept = BTreeSet::new();
        let public =
            http::build_client(std::sync::Arc::new(reqwest::cookie::Jar::default()), false)?;
        let mut enriched = BTreeMap::new();
        let mut publications = Publications::default();
        for account in accounts {
            // A selected refresh reads its receipts, then the other lists; saved page numbers cannot
            // locate moved cards reliably, so the Library stops where the last complete pass agrees.
            self.account(&account)?;
            let client = self.client(app, &account, false).await?;
            let mut sources: Vec<String> = owned_sources
                .iter()
                .filter(|e| {
                    e["accountId"] == account
                        && e["productId"].as_str().is_some_and(|p| ids.contains(p))
                })
                .filter_map(|e| e["source"].as_str().map(str::to_owned))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            sources.sort_by_key(|source| if source == "purchase" { 0 } else { 1 });
            for source in sources {
                let selected: BTreeSet<_> = owned_sources
                    .iter()
                    .filter(|e| e["accountId"] == account && e["source"] == source.as_str())
                    .filter_map(|e| e["productId"].as_str())
                    .filter(|id| ids.contains(*id))
                    .map(str::to_owned)
                    .collect();
                // Purchases settle from their own receipts and free items from their own pages;
                // only the Library and gifts are read as lists.
                let settled = match source.as_str() {
                    "purchase" => Some(
                        self.refresh_selected_purchase(
                            app,
                            job,
                            &account,
                            &client,
                            &selected,
                            &public,
                            &mut enriched,
                            &mut publications,
                            &mut left_products,
                            &mut kept,
                        )
                        .await?,
                    ),
                    "free" => {
                        self.refresh_selected_free(
                            app,
                            job,
                            &account,
                            &client,
                            &selected,
                            &public,
                            &mut enriched,
                            &mut publications,
                            &mut left_products,
                        )
                        .await?
                    }
                    _ => None,
                };
                if let Some(settled) = settled {
                    for product in &settled {
                        self.publish_ready_product(app, job, &mut publications, product)?;
                    }
                    continue;
                }
                let scan = self
                    .scan_source(
                        app,
                        job,
                        &account,
                        &source,
                        "full",
                        &client,
                        Some(&ids),
                        &public,
                        &mut enriched,
                        &mut publications,
                        None,
                    )
                    .await?;
                incomplete |= scan.incomplete();
                left_products.extend(scan.products);
                left_rows += scan.rows;
            }
        }
        self.publish_collected(app, job, &mut publications).await?;
        for id in &ids {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            if !enriched.contains_key(id) && !left_products.contains(id) {
                match self.enrich(&public, id).await {
                    // A refused or unreadable public page leaves the product as it was.
                    Err(error) if product_local_failure(&super::code(&error)) => {
                        left_products.insert(id.clone());
                    }
                    result => {
                        result?;
                        source_changed(app, std::iter::once(id.clone()));
                    }
                }
            }
        }
        left_products.extend(kept);
        record_incomplete(job, &left_products, left_rows);
        if incomplete {
            bail!("SYNC_INCOMPLETE");
        }
        Ok(())
    }
    async fn refresh_selected_purchase(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        client: &reqwest::Client,
        selected: &BTreeSet<String>,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        refused: &mut BTreeSet<String>,
        kept: &mut BTreeSet<String>,
    ) -> Result<BTreeSet<String>> {
        let acquisitions = self.inner.catalogue.list("acquisition")?;
        let remote = self.inner.catalogue.list("remote_file")?;
        let routes = selected_purchase_routes(&acquisitions, &remote, account, selected);
        // A product is settled only by every receipt it was bought on, read in full. Any other
        // keeps its account facts as they were: listing every order would only reach the same
        // receipts again. Its public page is still read at the end of the update.
        let mut left: BTreeSet<String> = selected
            .iter()
            .filter(|product| !routes.values().any(|expected| expected.contains(*product)))
            .cloned()
            .collect();
        let mut checkpoint = Checkpoint::fresh(account, "purchase", "selected")?;
        let mut collected = Vec::new();
        let mut row_routes = BTreeMap::new();
        let mut session_proven = false;
        for (route, expected) in routes {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            let page = match self.read_page(client, "purchase", &route, None).await {
                Ok(page) => page,
                // A receipt answers a lapsed session as refused; the order list says which it
                // was, and stops the job if the account must sign in again.
                Err(error) if product_local_failure(&super::code(&error)) => {
                    if !session_proven {
                        self.read_page(client, "purchase", "/orders?page=1", None)
                            .await?;
                        session_proven = true;
                    }
                    unreadable_page("purchase", &route, &super::code(&error))?
                }
                Err(error) => return Err(error),
            };
            checkpoint.details_completed += 1;
            let found: BTreeSet<_> = page
                .rows
                .iter()
                .filter(|row| parser::product_field_file_keys(row).is_some())
                .filter_map(|row| row.product_id.clone())
                .collect();
            // A row the receipt could not tie to a product may be one of those expected on it;
            // a row it did tie holds back only its own product, below.
            let tied = page
                .rows
                .iter()
                .filter(|row| row.product_id.is_some() && row.error_code.is_some())
                .count();
            if page.unresolved > tied {
                left.extend(expected);
                continue;
            }
            left.extend(expected.difference(&found).cloned());
            let journal_id = format!(
                "booth-selected-page:{}:{}",
                field(job, "id")?,
                safety::hash(&page.route)
            );
            let journal = json!({"version":1,"id":journal_id,"accountId":account,"source":"purchase","selected":true,"page":page});
            self.inner
                .catalogue
                .put("checkpoint", &journal_id, journal)?;
            for row in page.rows.iter().filter(|row| {
                row.product_id
                    .as_ref()
                    .is_some_and(|id| selected.contains(id))
            }) {
                row_routes.insert(row.source_record_id.clone(), page.route.clone());
                collected.push(row.clone());
            }
            job["progress"] = json!({"stage":"collecting","source":"purchase","detailsCompleted":checkpoint.details_completed,"mode":"selected"});
            self.progress(app, job)?;
        }
        for row in collected
            .iter()
            .filter(|row| !row.manifest_complete || row.error_code.is_some())
        {
            left.extend(row.product_id.clone());
        }
        collected.retain(|row| row.product_id.as_ref().is_some_and(|id| !left.contains(id)));
        kept.extend(left);
        self.stage_selected(
            app,
            job,
            account,
            "purchase",
            checkpoint,
            collected,
            row_routes,
            public,
            enriched,
            publications,
            refused,
        )
        .await
    }
    /// A free item's own page lists everything it offers now, and the download history a
    /// sync recorded proves the account has it, so its update reads that page and not the
    /// whole history. An offer that ended or a page that does not read in full goes back to
    /// the history list, as a sync reads it.
    async fn refresh_selected_free(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        client: &reqwest::Client,
        selected: &BTreeSet<String>,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        refused: &mut BTreeSet<String>,
    ) -> Result<Option<BTreeSet<String>>> {
        let acquisitions = self.inner.catalogue.list("acquisition")?;
        let histories: Vec<_> = selected
            .iter()
            .map(|product| (product, saved_free_history(&acquisitions, account, product)))
            .collect();
        if histories.iter().any(|(_, history)| history.is_empty()) {
            return Ok(None);
        }
        let mut checkpoint = Checkpoint::fresh(account, "free", "selected")?;
        // Item pages are public, so the history's first page is what tells a lapsed session:
        // the job stops there and asks for sign-in, as a sync would.
        let head = format!("{}?page=1", safety::source_path("free")?);
        self.read_page(client, "free", &head, None).await?;
        checkpoint.pages += 1;
        let mut rows = Vec::new();
        let mut row_routes = BTreeMap::new();
        for (product, history) in histories {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            let route = safety::free_item_route(product)?;
            let page = self.fetch_page(client, "free", &route, None).await?;
            checkpoint.details_completed += 1;
            let Some(evidence) = page.free_item.as_ref().filter(|evidence| {
                evidence.product_id == *product
                    && evidence.manifest_complete
                    && !evidence.unavailable
                    && evidence.error_code.is_none()
                    && !evidence.files.is_empty()
            }) else {
                return Ok(None);
            };
            for mut row in history {
                augment_free_row(&mut row, evidence);
                row_routes.insert(row.source_record_id.clone(), route.clone());
                rows.push(row);
            }
            job["progress"] = json!({"stage":"collecting","source":"free","detailsCompleted":checkpoint.details_completed,"mode":"selected"});
            self.progress(app, job)?;
        }
        self.stage_selected(
            app,
            job,
            account,
            "free",
            checkpoint,
            rows,
            row_routes,
            public,
            enriched,
            publications,
            refused,
        )
        .await
        .map(Some)
    }
    /// Prepares the products a selected refresh settled for publication, with fresh public
    /// metadata, and returns them.
    async fn stage_selected(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        source: &str,
        checkpoint: Checkpoint,
        rows: Vec<parser::Row>,
        row_routes: BTreeMap<String, String>,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        refused: &mut BTreeSet<String>,
    ) -> Result<BTreeSet<String>> {
        let key = selected_checkpoint_key(job, account, source)?;
        let products: BTreeSet<String> = rows
            .iter()
            .filter_map(|row| row.product_id.clone())
            .collect();
        for product in &products {
            let product_rows: Vec<_> = rows
                .iter()
                .filter(|row| row.product_id.as_ref() == Some(product))
                .collect();
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            for row in &product_rows {
                self.plan_row(
                    &mut batch,
                    account,
                    source,
                    row,
                    &row_routes[&row.source_record_id],
                    true,
                )?;
            }
            let mut draft = batch
                .get("product", product)?
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
            job["progress"] = json!({"stage":"enriching","source":source,"products":products.len(),"detailsCompleted":checkpoint.details_completed,"mode":"selected"});
            self.progress(app, job)?;
            let needs_enrichment = if let Some(observation) = enriched.get(product) {
                draft = observation.clone();
                false
            } else {
                match self.enrich_into(public, product, &mut draft, None).await {
                    Ok(()) => {
                        enriched.insert(product.clone(), draft.clone());
                        false
                    }
                    // A refused or unreadable public page keeps this product's metadata
                    // as it was; its own account facts still publish.
                    Err(error) if product_local_failure(&super::code(&error)) => {
                        refused.insert(product.clone());
                        true
                    }
                    Err(error) => return Err(error),
                }
            };
            let draft_id = format!("booth-product:{}:{product}", checkpoint.run_id);
            self.save_product_draft(&draft_id, json!({"id":draft_id,"accountId":account,"source":source,"runId":checkpoint.run_id,"productId":product,"product":draft,"enriched":!needs_enrichment}))?;
            let files = observed_file_ids(account, product_rows.iter().copied());
            publications
                .products
                .entry(product.clone())
                .or_default()
                .push(ProductPublication {
                    account: account.into(),
                    source: source.into(),
                    product: product.clone(),
                    rows: product_rows.into_iter().cloned().collect(),
                    routes: row_routes.clone(),
                    draft,
                    files,
                    checkpoint_key: key.clone(),
                    force_verify: true,
                    needs_enrichment,
                });
        }
        publications.checkpoints.insert(key, checkpoint);
        Ok(products)
    }
    async fn fetch_page(
        &self,
        client: &reqwest::Client,
        source: &str,
        route: &str,
        prefetch: Option<&PublicPrefetch>,
    ) -> Result<parser::Page> {
        let page = match self.read_page(client, source, route, prefetch).await {
            // A receipt or free item the store refused, moved or garbled stays as it was
            // and the source goes on. A listing page or an outage still stops the source.
            Err(error) if detail_route(route) && product_local_failure(&super::code(&error)) => {
                unreadable_page(source, route, &super::code(&error))?
            }
            page => page?,
        };
        crate::support_trace::page(
            source,
            page.is_detail,
            page.rows.len() as u64,
            page.unresolved as u64,
            page.terminal,
        );
        Ok(page)
    }
    async fn read_page(
        &self,
        client: &reqwest::Client,
        source: &str,
        route: &str,
        prefetch: Option<&PublicPrefetch>,
    ) -> Result<parser::Page> {
        if source == "free" {
            if let Some(id) = safety::free_item_id(route) {
                let url =
                    url::Url::parse(route).map_err(|_| anyhow::anyhow!("INVALID_FREE_ITEM_ID"))?;
                let (status, body) = self.inner.http.get(client, url).await?;
                return if matches!(status.as_u16(), 404 | 410) {
                    free_item::Evidence::unknown(id, "PUBLIC_METADATA_UNAVAILABLE", true).page()
                } else if status.is_success() {
                    let evidence = free_item::parse(&body, id)?;
                    if let Some(prefetch) = prefetch {
                        prefetch
                            .support_pages
                            .lock()
                            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                            .remember(&evidence, &body)?;
                    }
                    evidence.page()
                } else if status.is_client_error() {
                    unreadable_page(source, route, "SOURCE_HTTP_FAILED")
                } else {
                    bail!("SOURCE_HTTP_FAILED")
                };
            }
        }
        let (status, body) = self
            .inner
            .http
            .get(client, safety::route(route, source)?)
            .await?;
        if !status.is_success() {
            // A receipt the store refused is about that receipt alone.
            if detail_route(route) && status.is_client_error() {
                return unreadable_page(source, route, "SOURCE_HTTP_FAILED");
            }
            bail!("SOURCE_HTTP_FAILED");
        }
        let mut page = parser::parse(&body, source, route)?;
        if source == "free" {
            page.details = eligible_free_rows(&page)
                .filter_map(|row| row.product_id.as_deref())
                .map(safety::free_item_route)
                .collect::<Result<BTreeSet<_>>>()?
                .into_iter()
                .collect();
            page.fingerprint = safety::hash(format!(
                "{}:{}",
                page.fingerprint,
                serde_json::to_string(&page.details)
                    .map_err(|_| anyhow::anyhow!("FREE_ITEM_INVALID"))?
            ));
        }
        Ok(page)
    }
    /// BOOTH's gift list shows no dates; the item page builds its received notice from the
    /// JSON sent to the signed-in account. The date joins the row before the page is
    /// journaled, so it publishes with the grant and leaves the record ID alone.
    async fn read_gift_dates(
        &self,
        job: &Value,
        account: &str,
        mode: &str,
        client: &reqwest::Client,
        selected: Option<&BTreeSet<String>>,
        page: &mut parser::Page,
    ) -> Result<usize> {
        let products = self.undated_gift_products(account, mode, selected, page)?;
        for product in &products {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            let date = self.gift_accepted_date(client, product).await?;
            crate::support_trace::page("gift", true, 0, 0, false);
            let Some(date) = date else {
                crate::support_trace::source_reason("gift", "GIFT_DATE_UNKNOWN", 1);
                continue;
            };
            for row in &mut page.rows {
                if row.product_id.as_ref() == Some(product) {
                    row.acquired_date_text = Some(date.clone());
                }
            }
        }
        Ok(products.len())
    }
    /// An accepted date never changes, so a row that has one costs no request. A listing
    /// proven removed has nothing to say; like its public page, only Full rereads it.
    fn undated_gift_products(
        &self,
        account: &str,
        mode: &str,
        selected: Option<&BTreeSet<String>>,
        page: &parser::Page,
    ) -> Result<BTreeSet<String>> {
        let catalogue = &self.inner.catalogue;
        let known = |date: Option<&str>| date.is_some_and(|date| !date.trim().is_empty());
        let mut products = BTreeSet::new();
        for row in &page.rows {
            let Some(product) = row.product_id.as_deref().filter(|id| safety::numeric(id)) else {
                continue;
            };
            if known(row.acquired_date_text.as_deref())
                || selected.is_some_and(|ids| !ids.contains(product))
            {
                continue;
            }
            let saved = catalogue.get(
                "acquisition",
                &format!("{account}:{}", row.source_record_id),
            )?;
            if saved.is_some_and(|saved| known(saved["acquiredDateText"].as_str())) {
                continue;
            }
            if mode == "incremental"
                && catalogue
                    .get("product", product)?
                    .is_some_and(|saved| saved["publicPageAvailability"] == "unavailable")
            {
                continue;
            }
            products.insert(product.to_owned());
        }
        Ok(products)
    }
    /// A removed listing or a refused page leaves the date unknown for a later sync, without
    /// touching the gift's files or coverage. An outage or throttling still stops the job.
    async fn gift_accepted_date(
        &self,
        client: &reqwest::Client,
        id: &str,
    ) -> Result<Option<String>> {
        let url = url::Url::parse(&format!("https://booth.pm/ja/items/{id}.json"))
            .map_err(|_| anyhow::anyhow!("INVALID_PRODUCT_ID"))?;
        match self.inner.http.get(client, url).await {
            Ok((status, body)) if status.is_success() => Ok(serde_json::from_str(&body)
                .ok()
                .and_then(|value| metadata::gift_accepted_date(&value, id))),
            Ok(_) => Ok(None),
            Err(error) if product_local_failure(&super::code(&error)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    async fn scan_source(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        source: &str,
        mode: &str,
        client: &reqwest::Client,
        selected: Option<&BTreeSet<String>>,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        prefetch: Option<&PublicPrefetch>,
    ) -> Result<SourceScan> {
        crate::support_trace::source_start(source);
        let result = self
            .scan_source_inner(
                app,
                job,
                account,
                source,
                mode,
                client,
                selected,
                public,
                enriched,
                publications,
                prefetch,
            )
            .await;
        let error = result.as_ref().err().map(super::code);
        crate::support_trace::source_done(
            source,
            result.as_ref().ok().map(|scan| scan.settled),
            error.as_deref(),
        );
        result
    }
    async fn scan_source_inner(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        source: &str,
        mode: &str,
        client: &reqwest::Client,
        selected: Option<&BTreeSet<String>>,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        prefetch: Option<&PublicPrefetch>,
    ) -> Result<SourceScan> {
        let key = format!("booth:{account}:{source}");
        let read = SourceRead {
            app,
            account,
            source,
            mode,
            client,
            selected,
            prefetch,
            key: &key,
        };
        let baseline_key = format!("booth-complete:{account}:{source}");
        let previous_baseline = self
            .inner
            .catalogue
            .get("checkpoint", &baseline_key)?
            .and_then(|v| serde_json::from_value::<Baseline>(v).ok());
        let baseline = previous_baseline
            .as_ref()
            .filter(|b| b.usable(account, source));
        let known_receipts = previous_baseline
            .as_ref()
            .filter(|_| mode == "incremental" && selected.is_none())
            .map(|b| b.known_receipts(account, source))
            .unwrap_or_default();
        let mut checkpoint = self
            .resume_checkpoint(&key, account, source, mode, client, selected, prefetch)
            .await?;
        let mut tracker = pass_tracker(mode, selected, source, &checkpoint, baseline);
        let mut updated_tracker: Option<UpdatedTracker> = None;
        let mut free_history = self.replay_journals(
            &checkpoint,
            account,
            source,
            baseline,
            &mut tracker,
            &mut updated_tracker,
        )?;
        let coverage_id = self.start_source_pass(
            read,
            &mut checkpoint,
            &known_receipts,
            &tracker,
            &previous_baseline,
        )?;
        self.read_pass_pages(
            read,
            job,
            &previous_baseline,
            baseline,
            &known_receipts,
            &mut checkpoint,
            &mut tracker,
            &mut updated_tracker,
            &mut free_history,
        )
        .await?;
        let (head_route, final_head) = self.reread_source_head(read, job, &mut checkpoint).await?;
        // Replace the accessible manifest only after its current item rows were durably read.
        // Earlier acquisition observations/artifacts remain; retired remote files are not deleted.
        let PassRows {
            current_rows,
            updated_rows,
            free_evidence,
            mut row_routes,
            unbound_membership,
            terminal_unproven_observed,
            incomplete_products,
            availability_products,
            mut left_products,
            unresolved_rows,
        } = self
            .collect_pass_rows(
                job,
                account,
                source,
                &checkpoint,
                &free_history,
                publications,
            )
            .await?;
        let (all_rows, refreshed_products, retained_observations) = merge_pass_rows(
            &checkpoint,
            baseline,
            &known_receipts,
            source,
            &head_route,
            current_rows,
            &updated_rows,
            &mut row_routes,
        )?;
        let (manifests, mut rows_by_product) =
            product_manifests(&all_rows, selected, account, source);
        checkpoint.forget_unlisted(|id| rows_by_product.contains_key(id));
        let records = vec![];
        let closed_traversal = checkpoint.terminal
            && checkpoint.pending.is_empty()
            && !checkpoint.head.is_empty()
            && checkpoint.head == final_head.fingerprint;
        let mut enrichment_error = None;
        let metadata_total = manifests.len();
        let mut metadata_completed = checkpoint.published_products.len();
        self.note_source_collected(read, job, &checkpoint, unbound_membership, closed_traversal)?;
        let pass = PassProducts {
            head_route: &head_route,
            row_routes: &row_routes,
            free_evidence: &free_evidence,
            incomplete_products: &incomplete_products,
            availability_products: &availability_products,
            refreshed_products: &refreshed_products,
            retained_observations,
            closed_traversal,
            unbound_membership,
            metadata_total,
        };
        for (index, (product, (files, complete))) in manifests.into_iter().enumerate() {
            if index % 16 == 0 {
                tokio::task::yield_now().await;
                if self.cancelled(job)? {
                    bail!("CANCELLED");
                }
            }
            self.assemble_pass_product(
                read,
                job,
                public,
                enriched,
                publications,
                pass,
                &mut checkpoint,
                &mut rows_by_product,
                &mut left_products,
                &mut metadata_completed,
                &mut enrichment_error,
                product,
                files,
                complete,
            )
            .await?;
        }
        drop(rows_by_product);
        self.close_source_pass(
            read,
            job,
            publications,
            &previous_baseline,
            &final_head,
            PassClose {
                checkpoint,
                all_rows,
                updated_rows,
                records,
                baseline_key,
                coverage_id,
                left_products,
                incomplete_products,
                availability_products,
                enrichment_error,
                retained_observations,
                unbound_membership,
                closed_traversal,
                terminal_unproven_observed,
                unresolved_rows,
            },
        )
    }

    /// Reads the source's first page again once the pass is read, and moves the job on to
    /// enriching.
    async fn reread_source_head(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        checkpoint: &mut Checkpoint,
    ) -> Result<(String, parser::Page)> {
        let SourceRead {
            app,
            source,
            client,
            prefetch,
            ..
        } = read;
        // A source that shifted during the pass cannot be labelled complete.
        let head_route = format!("{}?page=1", safety::source_path(source)?);
        let final_head = self
            .fetch_page(client, source, &head_route, prefetch)
            .await?;
        if self.cancelled(job)? {
            bail!("CANCELLED");
        }
        record_source_head(checkpoint, &final_head);
        if let Some(prefetch) = prefetch {
            let early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            set_collecting_totals(job, &early, source);
        }
        job["progress"]["stage"] = json!("enriching");
        self.progress(app, job)?;
        Ok((head_route, final_head))
    }

    /// With prefetch, tells the early publication this source is collected; it publishes
    /// once every source is.
    fn note_source_collected(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        checkpoint: &Checkpoint,
        unbound_membership: bool,
        closed_traversal: bool,
    ) -> Result<()> {
        let SourceRead {
            app,
            account,
            source,
            prefetch,
            key,
            ..
        } = read;
        if let Some(prefetch) = prefetch {
            let mut early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            early.remaining_sources.remove(source);
            if unbound_membership || !closed_traversal {
                early
                    .publications
                    .unbound_sources
                    .insert((account.into(), source.into()));
            }
            early
                .publications
                .checkpoints
                .insert(key.to_owned(), checkpoint.clone());
            if early.remaining_sources.is_empty() {
                self.publish_early(app, job, &mut early, None)?;
            }
        }
        Ok(())
    }

    /// Requeues the receipts the previous pass no longer supplies, reuses the ones it does,
    /// saves the checkpoint and marks the source's coverage running.
    fn start_source_pass(
        &self,
        read: SourceRead<'_>,
        checkpoint: &mut Checkpoint,
        known_receipts: &BTreeMap<String, Vec<parser::Row>>,
        tracker: &Option<Tracker>,
        previous_baseline: &Option<Baseline>,
    ) -> Result<String> {
        let SourceRead {
            account,
            source,
            mode,
            selected,
            key,
            ..
        } = read;
        // A resumed run, including one interrupted by an App update, skips known
        // receipts still queued. One the previous pass no longer supplies is read.
        let unsupplied: Vec<_> = checkpoint
            .reused
            .iter()
            .filter(|route| !known_receipts.contains_key(*route))
            .cloned()
            .collect();
        for route in unsupplied {
            checkpoint.reused.remove(&route);
            checkpoint.pending.push(route);
        }
        reuse_known_receipts(checkpoint, known_receipts);
        self.save_checkpoint(key, checkpoint)?;
        self.inner
            .catalogue
            .prune_store_checkpoints("BOOTH", account)?;
        let coverage_id = format!("{account}:{source}");
        if let Some((kind, id, value)) = source_coverage_record(
            selected,
            &coverage_id,
            json!({"id":coverage_id,"accountId":account,"source":source,"status":"running","mode":mode,"freeHistoryStartsAt":if source=="free"{Some("2026-06-15")}else{None},"historicalComplete":false,"incrementalBoundary":if tracker.is_some(){"awaitingNewestFirstOverlap"}else{"fullTraversalWhenOrderingUnproven"},"lastFullTraversalAt":previous_baseline.as_ref().map(|b| &b.full_traversal_at),"updatedAt":now()}),
        ) {
            self.inner.catalogue.put(&kind, &id, value)?;
        }
        Ok(coverage_id)
    }

    /// Reads the source's pending pages, several at a time, until none is left.
    async fn read_pass_pages(
        &self,
        source_read: SourceRead<'_>,
        job: &mut Value,
        previous_baseline: &Option<Baseline>,
        baseline: Option<&Baseline>,
        known_receipts: &BTreeMap<String, Vec<parser::Row>>,
        checkpoint: &mut Checkpoint,
        tracker: &mut Option<Tracker>,
        updated_tracker: &mut Option<UpdatedTracker>,
        free_history: &mut BTreeMap<String, Vec<parser::Row>>,
    ) -> Result<()> {
        let SourceRead {
            source,
            client,
            prefetch,
            ..
        } = source_read;
        let mut reads = FuturesUnordered::new();
        let mut active_routes = BTreeSet::new();
        while !checkpoint.pending.is_empty() {
            if checkpoint.pages + checkpoint.details_completed >= 10000
                || checkpoint.pending.len() > 50000
            {
                bail!("SOURCE_TRAVERSAL_LIMIT");
            }
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            let routes = available_routes(&checkpoint.pending, &active_routes)?;
            for route in routes {
                if let Some(id) = safety::free_item_id(&route) {
                    if source != "free" || !free_history.contains_key(id) {
                        bail!("FREE_ITEM_HISTORY_UNPROVEN");
                    }
                }
                active_routes.insert(route.clone());
                reads.push(async move {
                    let read = self.fetch_page(client, source, &route, prefetch).await;
                    (route, read)
                });
            }
            if let Some((route, read)) = reads.next().await {
                active_routes.remove(&route);
                self.read_source_page(
                    source_read,
                    job,
                    route,
                    read,
                    previous_baseline,
                    baseline,
                    known_receipts,
                    checkpoint,
                    tracker,
                    updated_tracker,
                    free_history,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Stages the pass's checkpoint, baseline and coverage, hands them to publication and
    /// settles what the source left.
    fn close_source_pass(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        publications: &mut Publications,
        previous_baseline: &Option<Baseline>,
        final_head: &parser::Page,
        pass: PassClose,
    ) -> Result<SourceScan> {
        let SourceRead {
            app,
            account,
            source,
            mode,
            selected,
            prefetch,
            key,
            ..
        } = read;
        let PassClose {
            mut checkpoint,
            all_rows,
            updated_rows,
            mut records,
            baseline_key,
            coverage_id,
            mut left_products,
            incomplete_products,
            availability_products,
            enrichment_error,
            retained_observations,
            unbound_membership,
            closed_traversal,
            terminal_unproven_observed,
            unresolved_rows,
        } = pass;
        // Collection closure and publication completion are distinct. Pending
        // prepared products remain checkpoint work until their atomic commit.
        let collected_complete =
            collection_membership_complete(&checkpoint, &publications.incomplete_scopes);
        let full_traversal_at =
            if retained_observations || !collected_complete || selected.is_some() {
                previous_baseline
                    .as_ref()
                    .map(|b| b.full_traversal_at.clone())
                    .unwrap_or_default()
            } else {
                checkpoint.read_from()
            };
        // A product bought more than once can list several Library rows under one record ID.
        if collected_complete
            && selected.is_none()
            && (source == "library" || super::incremental::unique(&all_rows))
        {
            checkpoint
                .product_ids
                .extend(all_rows.iter().filter_map(|r| r.product_id.clone()));
            let mut details: BTreeSet<String> = checkpoint
                .committed
                .keys()
                .filter(|route| route.strip_prefix("/orders/").is_some_and(safety::numeric))
                .cloned()
                .collect();
            details.extend(checkpoint.reused.iter().cloned());
            let base = Baseline {
                version: 1,
                account_id: account.into(),
                source: source.into(),
                complete: true,
                ordering: checkpoint.ordering,
                rows: all_rows,
                full_traversal_at: full_traversal_at.clone(),
                details,
                updated_head: super::incremental::next_head(
                    &updated_rows,
                    previous_baseline
                        .as_ref()
                        .map_or(&[], |base| &base.updated_head),
                ),
            };
            let mut value = serde_json::to_value(base)?;
            value["id"] = json!(baseline_key);
            records.push(("checkpoint".into(), baseline_key.clone(), value));
        }
        checkpoint.finished = false;
        let mut finished = serde_json::to_value(&checkpoint)?;
        finished["id"] = json!(key);
        records.push(("checkpoint".into(), key.to_owned(), finished));
        records.extend(source_coverage_record(selected, &coverage_id, json!({"id":coverage_id,"accountId":account,"source":source,"status":if checkpoint.complete(){"complete"}else{"incomplete"},"coverageComplete":checkpoint.complete(),"terminalSeen":checkpoint.terminal,"unresolvedRows":checkpoint.unresolved,"availabilityWarnings":availability_products.len(),"pages":checkpoint.pages,"detailsCompleted":checkpoint.details_completed,"requestsCompleted":checkpoint.pages+checkpoint.details_completed,"productCount":checkpoint.product_ids.len(),"mode":mode,"freeHistoryStartsAt":if source=="free"{Some("2026-06-15")}else{None},"historicalComplete":false,"giftEventLedgerComplete":false,"incrementalBoundary":if checkpoint.overlap_end.is_some(){"verifiedNewestFirstStableOverlap"}else{"fullTraversalWhenOrderingUnproven"},"lastFullTraversalAt":full_traversal_at,"ordering":checkpoint.ordering,"completedAt":now()})));
        // Advance baseline, completion and manifests together: a crash cannot resume
        // an old prefix against a newly replaced baseline.
        let mut collected = serde_json::to_value(&checkpoint)?;
        collected["id"] = json!(key);
        let mut coverage = records
            .iter()
            .find(|(kind, _, _)| kind == "source_coverage")
            .map(|(_, _, value)| value.clone());
        let mut staged = vec![("checkpoint".into(), key.to_owned(), collected)];
        if let Some(coverage) = &mut coverage {
            coverage["status"] = json!("collected");
            coverage["coverageComplete"] = json!(false);
            coverage.as_object_mut().unwrap().remove("completedAt");
            staged.push(("source_coverage".into(), coverage_id, coverage.clone()));
        }
        self.inner.catalogue.put_many(staged)?;
        if unbound_membership || !closed_traversal {
            publications
                .unbound_sources
                .insert((account.into(), source.into()));
        }
        publications
            .checkpoints
            .insert(key.to_owned(), checkpoint.clone());
        publications.source_closures.insert(
            (account.into(), source.into()),
            (
                unbound_membership,
                !checkpoint.head.is_empty() && checkpoint.head == final_head.fingerprint,
            ),
        );
        trace_source_assembly(&checkpoint, publications, terminal_unproven_observed);
        publications.final_records.extend(records);
        if prefetch.is_none() {
            job["progress"]["stage"] = json!("finalizing");
            self.progress(app, job)?;
        }
        if let Some(error_code) = enrichment_error {
            bail!(error_code);
        }
        // A bound row without files is in no manifest but is an asset left as it was too.
        left_products.extend(
            incomplete_products
                .iter()
                .filter(|product| selected.is_none_or(|ids| ids.contains(*product)))
                .cloned(),
        );
        let gaps_local = source_gaps_local(
            &checkpoint,
            &final_head.fingerprint,
            terminal_unproven_observed,
            !left_products.is_empty(),
            unresolved_rows,
        );
        Ok(SourceScan {
            settled: collection_settled(
                &checkpoint,
                collected_complete,
                closed_traversal,
                unbound_membership || !incomplete_products.is_empty(),
                availability_products.len(),
            ),
            gaps_local,
            products: left_products,
            rows: unresolved_rows,
        })
    }

    /// Assembles one listed product from its rows: a published one is restored, any other
    /// is planned, enriched and kept for publication.
    async fn assemble_pass_product(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        public: &reqwest::Client,
        enriched: &mut BTreeMap<String, Value>,
        publications: &mut Publications,
        pass: PassProducts<'_>,
        checkpoint: &mut Checkpoint,
        rows_by_product: &mut BTreeMap<&str, Vec<&parser::Row>>,
        left_products: &mut BTreeSet<String>,
        metadata_completed: &mut usize,
        enrichment_error: &mut Option<String>,
        product: String,
        files: BTreeSet<String>,
        complete: bool,
    ) -> Result<()> {
        let SourceRead {
            app,
            account,
            source,
            mode,
            prefetch,
            ..
        } = read;
        let PassProducts {
            head_route,
            row_routes,
            free_evidence,
            incomplete_products,
            availability_products,
            refreshed_products,
            retained_observations,
            closed_traversal,
            unbound_membership,
            metadata_total,
        } = pass;
        if retained_observations && !refreshed_products.contains(&product) {
            return Ok(()); // The untouched tail retains its actual prior observation time.
        }
        let already_published = checkpoint.published_products.contains(&product);
        let manifest_complete = complete
            && !incomplete_products.contains(&product)
            && (free_evidence
                .get(&product)
                .is_some_and(|e| e.manifest_complete)
                || (closed_traversal && !unbound_membership));
        let draft_id = format!("booth-product:{}:{product}", checkpoint.run_id);
        let product_rows = rows_by_product
            .remove(product.as_str())
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let availability_only = source == "free"
            && availability_products.contains(&product)
            && !incomplete_products.contains(&product);
        // This asset's own rows or item page could not be read in full.
        if !availability_only && (!complete || incomplete_products.contains(&product)) {
            left_products.insert(product.clone());
        }
        let observed_rows =
            observed_grant_rows(source, &product_rows, manifest_complete, availability_only);
        let observed_files = observed_file_ids(account, observed_rows.iter().copied());
        let saved_draft = self.inner.catalogue.get("checkpoint", &draft_id)?;
        if !availability_only
            && self.restore_product_publication(
                &product,
                &product_rows,
                row_routes,
                &files,
                manifest_complete,
                saved_draft.as_ref(),
                checkpoint,
            )?
        {
            self.keep_restored_product(
                read,
                publications,
                checkpoint,
                row_routes,
                metadata_completed,
                already_published,
                &product,
                &product_rows,
                files,
                &saved_draft,
            )?;
            return Ok(());
        }
        let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
        for row in &product_rows {
            let route = row_routes
                .get(&row.source_record_id)
                .map(String::as_str)
                .unwrap_or(head_route);
            self.plan_row_mode(
                &mut batch,
                account,
                source,
                row,
                route,
                mode != "incremental",
                availability_only,
                None,
            )?;
        }
        let mut draft = batch
            .get("product", &product)?
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let mut needs_enrichment = checkpoint.enrichment_pending.contains(&product);
        if let Some(saved) = saved_draft.filter(|s| {
            product_draft_scope(s, account, source, &checkpoint.run_id, &product)
                && s["enriched"] == true
        }) {
            draft = saved["product"].clone();
            needs_enrichment = false;
        } else if let Some(observation) = enriched.get(&product) {
            draft = observation.clone();
            needs_enrichment = false;
        } else if checkpoint.enrichment_pending.contains(&product) && prefetch.is_none() {
            job["progress"] = json!({"stage":"enriching","source":source,"pages":checkpoint.pages,"detailsCompleted":checkpoint.details_completed,"requestsCompleted":checkpoint.pages+checkpoint.details_completed,"products":checkpoint.product_ids.len(),"remaining":checkpoint.enrichment_pending.len(),"published":checkpoint.published_products.len(),"metadataCompleted":metadata_completed,"metadataTotal":metadata_total});
            self.progress(app, job)?;
            if let Err(error) = self.enrich_into(public, &product, &mut draft, None).await {
                let error_code = super::code(&error);
                self.save_product_draft(&draft_id,
                    json!({"id":draft_id,"accountId":account,"source":source,"runId":checkpoint.run_id,"productId":product,"product":draft,"enriched":false,"errorCode":error_code}))?;
                // A refused or unreadable public page leaves this product as it was.
                if product_local_failure(&error_code) {
                    left_products.insert(product.clone());
                } else {
                    enrichment_error.get_or_insert(error_code);
                }
                return Ok(());
            }
            enriched.insert(product.clone(), draft.clone());
            needs_enrichment = false;
        }
        if needs_enrichment {
            if let Some(prefetch) = prefetch {
                let first = prefetch.enqueue_once(&product)?;
                if first {
                    prefetch
                        .requests
                        .send(PublicRequest {
                            product: product.clone(),
                            draft: draft.clone(),
                            account: account.into(),
                            source: source.into(),
                            run_id: checkpoint.run_id.clone(),
                        })
                        .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_FAILED"))?;
                }
            }
        }
        if needs_enrichment && prefetch.is_some() {
            let request = PublicRequest {
                product: product.clone(),
                draft: draft.clone(),
                account: account.into(),
                source: source.into(),
                run_id: checkpoint.run_id.clone(),
            };
            if let Some(saved) = self
                .inner
                .catalogue
                .get("checkpoint", &draft_id)?
                .and_then(|saved| request.saved_product(&saved))
            {
                draft = saved;
                needs_enrichment = false;
            }
        }
        self.save_product_draft(&draft_id,
            json!({"id":draft_id,"accountId":account,"source":source,"runId":checkpoint.run_id,"productId":product,"product":draft,"enriched":!needs_enrichment,"errorCode":if manifest_complete{Value::Null}else{json!("SYNC_INCOMPLETE")}}))?;
        if self.cancelled(job)? {
            bail!("CANCELLED");
        }
        *metadata_completed += 1;
        if !manifest_complete && !availability_only {
            checkpoint.enrichment_pending.insert(product.clone());
            if let Some(prefetch) = prefetch {
                prefetch
                    .early
                    .lock()
                    .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                    .publications
                    .incomplete_scopes
                    .insert((account.into(), source.into(), product.clone()));
            }
            publications
                .incomplete_scopes
                .insert((account.into(), source.into(), product.clone()));
            if observed_rows.is_empty() {
                return Ok(());
            }
        }
        self.report_product_progress(read, job, checkpoint, *metadata_completed, metadata_total)?;
        self.keep_pass_product(
            read,
            job,
            publications,
            checkpoint,
            row_routes,
            product,
            observed_rows,
            observed_files,
            draft,
            needs_enrichment,
            availability_only,
        )?;
        Ok(())
    }

    /// Shows the assembled product in the job's progress and moves the job on to enriching.
    fn report_product_progress(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        checkpoint: &Checkpoint,
        metadata_completed: usize,
        metadata_total: usize,
    ) -> Result<()> {
        let SourceRead {
            app,
            source,
            prefetch,
            ..
        } = read;
        job["progress"] = json!({"stage":if prefetch.is_some(){"collecting"}else{"publishing"},"source":source,"pages":checkpoint.pages,"detailsCompleted":checkpoint.details_completed,"requestsCompleted":checkpoint.pages+checkpoint.details_completed,"products":checkpoint.product_ids.len(),"remaining":checkpoint.enrichment_pending.len(),"published":checkpoint.published_products.len(),"metadataCompleted":metadata_completed,"metadataTotal":metadata_total});
        if let Some(prefetch) = prefetch {
            let early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            set_collecting_totals(job, &early, source);
        }
        job["progress"]["stage"] = json!("enriching");
        self.progress(app, job)?;
        Ok(())
    }

    /// Keeps a product the pass already published, from its enriched draft or its record.
    fn keep_restored_product(
        &self,
        read: SourceRead<'_>,
        publications: &mut Publications,
        checkpoint: &Checkpoint,
        row_routes: &BTreeMap<String, String>,
        metadata_completed: &mut usize,
        already_published: bool,
        product: &str,
        product_rows: &[&parser::Row],
        files: BTreeSet<String>,
        saved_draft: &Option<Value>,
    ) -> Result<()> {
        let SourceRead {
            account,
            source,
            mode,
            prefetch,
            key,
            ..
        } = read;
        if !already_published {
            *metadata_completed += 1;
        }
        let draft = if let Some(saved) = saved_draft.as_ref().filter(|saved| {
            product_draft_scope(saved, account, source, &checkpoint.run_id, product)
                && saved["enriched"] == true
        }) {
            saved["product"].clone()
        } else {
            self.inner
                .catalogue
                .get("product", product)?
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?
        };
        self.retain_published_context(
            publications,
            prefetch,
            ProductPublication {
                account: account.into(),
                source: source.into(),
                product: product.to_owned(),
                rows: product_rows.iter().map(|row| (*row).clone()).collect(),
                routes: product_rows
                    .iter()
                    .filter_map(|row| {
                        row_routes
                            .get(&row.source_record_id)
                            .map(|route| (row.source_record_id.clone(), route.clone()))
                    })
                    .collect(),
                draft,
                files,
                checkpoint_key: key.to_owned(),
                force_verify: mode != "incremental",
                needs_enrichment: false,
            },
        )?;
        Ok(())
    }

    /// Marks the product pending, then keeps it for publication: an availability-only grant
    /// beside its product, any other among the pass's products and, with prefetch, published
    /// early.
    fn keep_pass_product(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        publications: &mut Publications,
        checkpoint: &mut Checkpoint,
        row_routes: &BTreeMap<String, String>,
        product: String,
        observed_rows: Vec<&parser::Row>,
        observed_files: BTreeSet<String>,
        draft: Value,
        needs_enrichment: bool,
        availability_only: bool,
    ) -> Result<()> {
        let SourceRead {
            app,
            account,
            source,
            mode,
            prefetch,
            key,
            ..
        } = read;
        checkpoint.enrichment_pending.insert(product.clone());
        let product_routes = observed_rows
            .iter()
            .filter_map(|row| {
                row_routes
                    .get(&row.source_record_id)
                    .map(|route| (row.source_record_id.clone(), route.clone()))
            })
            .collect();
        let entry = ProductPublication {
            account: account.into(),
            source: source.into(),
            product: product.clone(),
            rows: observed_rows.into_iter().cloned().collect(),
            routes: product_routes,
            draft,
            files: observed_files,
            checkpoint_key: key.to_owned(),
            force_verify: mode != "incremental",
            needs_enrichment,
        };
        if availability_only {
            publications
                .availability
                .entry(product.clone())
                .or_default()
                .push(entry);
        } else {
            remember_product_context(publications, entry);
        }
        if let Some(prefetch) = prefetch.filter(|_| !availability_only) {
            let mut early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            let entry = publications.products[&product].last().unwrap().clone();
            remember_product_context(&mut early.publications, entry);
            self.publish_early(app, job, &mut early, Some(&BTreeSet::from([product])))?;
        }
        Ok(())
    }

    /// Rereads the pass's journaled pages: the rows they listed, in page order, and what
    /// they left unproven.
    async fn collect_pass_rows(
        &self,
        job: &Value,
        account: &str,
        source: &str,
        checkpoint: &Checkpoint,
        free_history: &BTreeMap<String, Vec<parser::Row>>,
        publications: &mut Publications,
    ) -> Result<PassRows> {
        let mut current_rows = vec![];
        let mut updated_rows = vec![];
        let mut free_evidence = BTreeMap::new();
        let mut row_routes = BTreeMap::new();
        let mut free_routes = BTreeMap::new();
        let mut unbound_membership = false;
        let mut terminal_unproven_observed = false;
        let mut incomplete_products = BTreeSet::new();
        let mut availability_products = BTreeSet::new();
        // Assets and rows left as they were. They no longer hold the job back, but an
        // unbound row still keeps this source's membership uncertified.
        let (left_products, mut unresolved_rows) = (BTreeSet::new(), 0);
        let pass_started = checkpoint.read_from();
        for (index, journal_id) in checkpoint.journal_ids.iter().enumerate() {
            if index % 16 == 0 {
                // Scanners and public metadata share one try_join task. Let the
                // sibling future run during this synchronous assembly work.
                tokio::task::yield_now().await;
                if self.cancelled(job)? {
                    bail!("CANCELLED");
                }
            }
            let journal = self
                .inner
                .catalogue
                .get("checkpoint", journal_id)?
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
            let page = checked_journal_page(&journal, account, source, &checkpoint.run_id)?;
            // A row listed twice keeps its older place.
            for (record, position) in free_history_positions(&page) {
                if let Some(order) = free_history_order(&pass_started, position) {
                    let saved = publications
                        .free_orders
                        .entry(format!("{account}:{record}"))
                        .or_insert_with(|| order.clone());
                    if order < *saved {
                        *saved = order;
                    }
                }
            }
            // A row read in updated order joins through its own record. If the App went on to
            // read every page, those pages hold it already, so such a page counts for nothing.
            if safety::updated_order(&page.route) {
                if checkpoint.overlap_end.is_some() {
                    updated_rows.extend(page.rows);
                }
                continue;
            }
            let mut page_unbound = false;
            // Normal parser unresolved rows are either exact product-bound
            // failures or unbound controls/routes. Free item evidence has its
            // own complete product-file-list scope.
            if page.unresolved != 0 {
                if let Some(evidence) = &page.free_item {
                    if settled_free_absence(&page, free_history) {
                        availability_products.insert(evidence.product_id.clone());
                    } else {
                        incomplete_products.insert(evidence.product_id.clone());
                    }
                } else if !page.rows.is_empty()
                    && page
                        .rows
                        .iter()
                        .filter(|row| {
                            row.error_code.as_deref() != Some("BOOTH_WITHHELD_PRODUCT_ID")
                        })
                        .all(|row| row.product_id.is_some())
                    && page.rows.iter().any(|row| row.error_code.is_some())
                {
                    incomplete_products.extend(
                        page.rows
                            .iter()
                            .filter(|row| row.error_code.is_some())
                            .filter_map(|row| row.product_id.clone()),
                    );
                } else {
                    unbound_membership = true;
                    page_unbound = true;
                    unresolved_rows += unbound_row_count(&page);
                }
            }
            if !page.is_detail && !page.terminal && page.next.is_empty() {
                unbound_membership = true;
                page_unbound = true;
            }
            terminal_unproven_observed |= trace_checked_journal(&page, page_unbound);
            for row in &page.rows {
                row_routes.insert(row.source_record_id.clone(), page.route.clone());
            }
            if let Some(evidence) = page.free_item {
                free_routes.insert(evidence.product_id.clone(), page.route);
                free_evidence.insert(evidence.product_id.clone(), evidence);
            }
            current_rows.extend(page.rows);
        }
        if source == "free" {
            for row in &mut current_rows {
                if let Some(evidence) = row.product_id.as_ref().and_then(|id| free_evidence.get(id))
                {
                    augment_free_row(row, evidence);
                    if let Some(route) = free_routes.get(&evidence.product_id) {
                        row_routes.insert(row.source_record_id.clone(), route.clone());
                    }
                }
            }
        }
        Ok(PassRows {
            current_rows,
            updated_rows,
            free_evidence,
            row_routes,
            unbound_membership,
            terminal_unproven_observed,
            incomplete_products,
            availability_products,
            left_products,
            unresolved_rows,
        })
    }

    /// Takes one fetched page: journals it, queues what it links, moves the trackers, and
    /// publishes what it discovered early.
    async fn read_source_page(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        route: String,
        read_page: Result<parser::Page>,
        previous_baseline: &Option<Baseline>,
        baseline: Option<&Baseline>,
        known_receipts: &BTreeMap<String, Vec<parser::Row>>,
        checkpoint: &mut Checkpoint,
        tracker: &mut Option<Tracker>,
        updated_tracker: &mut Option<UpdatedTracker>,
        free_history: &mut BTreeMap<String, Vec<parser::Row>>,
    ) -> Result<()> {
        let SourceRead {
            app,
            account,
            source,
            mode,
            client,
            selected,
            prefetch,
            key,
        } = read;
        let mut page = match read_page {
            Err(error) if refused_updated_order(&route, &error) => {
                crate::support_trace::source_reason(source, &super::code(&error), 1);
                read_every_page(checkpoint);
                *tracker = None;
                *updated_tracker = None;
                self.save_checkpoint(key, checkpoint)?;
                return Ok(());
            }
            read => read?,
        };
        if contradicts_last_pass(&page, previous_baseline.as_ref()) {
            page.unresolved += 1;
        }
        if self.cancelled(job)? {
            bail!("CANCELLED");
        }
        if source == "gift" {
            checkpoint.details_completed += self
                .read_gift_dates(job, account, mode, client, selected, &mut page)
                .await?;
        }
        // Persist only parsed digital projections. Replaying this journal is idempotent.
        let journal_id = format!(
            "booth-page:{}:{}",
            checkpoint.run_id,
            safety::hash(format!("{}:{}", page.route, page.fingerprint))
        );
        let journal = json!({"version":1,"id":journal_id,"accountId":account,"source":source,"runId":checkpoint.run_id,"page":page});
        remember_free_history(&page, free_history);
        if let Some(evidence) = &page.free_item {
            free_history
                .get(&evidence.product_id)
                .ok_or_else(|| anyhow::anyhow!("FREE_ITEM_HISTORY_UNPROVEN"))?;
        }
        // In updated order only what changed counts as read; the rest is the last pass's.
        let unchanged_here = |row: &parser::Row| {
            safety::updated_order(&page.route)
                && !updated_tracker
                    .as_ref()
                    .is_some_and(|updated| updated.changes(row))
        };
        for row in page.rows.iter().filter(|row| !unchanged_here(row)) {
            if let Some(id) = &row.product_id {
                checkpoint.product_ids.insert(id.clone());
            }
            if selected
                .is_some_and(|ids| row.product_id.as_ref().is_some_and(|id| !ids.contains(id)))
            {
                continue;
            }
            if let Some(id) = &row.product_id {
                checkpoint.product_ids.insert(id.clone());
                if mode != "incremental"
                    || public_refresh_needed(self.inner.catalogue.get("product", id)?.as_ref())
                {
                    checkpoint.enrichment_pending.insert(id.clone());
                }
            }
        }
        record_source_page(checkpoint, source, &route, &page, selected)?;
        reuse_known_receipts(checkpoint, known_receipts);
        if let Some(limit) = crate::verification::source_page_limit() {
            limit_listing_pages(checkpoint, limit);
        }
        checkpoint.journal_ids.push(journal_id.clone());
        let updated_page = safety::updated_order(&page.route);
        if updated_page {
            match updated_tracker
                .as_mut()
                .map(|updated| updated.observe(&page))
            {
                Some(UpdatedRead::Settled) => checkpoint.pending.clear(),
                Some(UpdatedRead::Reading) => {}
                Some(UpdatedRead::Broken) | None => {
                    read_every_page(checkpoint);
                    *tracker = None;
                    *updated_tracker = None;
                }
            }
        } else if let Some(end) = tracker
            .as_mut()
            .and_then(|t| t.observe(&page))
            .filter(|_| !page.terminal)
        {
            checkpoint.overlap_end = Some(end);
            checkpoint.pending = vec![safety::updated_route(1)];
            // This terminal belongs to the previous complete tail, not this page.
            checkpoint.terminal = true;
            *updated_tracker = tracker
                .as_ref()
                .zip(baseline)
                .map(|(tracker, base)| tracker.updated(&base.updated_head));
        }
        if !updated_page && !page.is_detail && !page.terminal && page.next.is_empty() {
            checkpoint.unresolved += 1;
        }
        self.commit_source_page(&journal_id, &journal, key, checkpoint)?;
        if let Some(prefetch) = prefetch {
            self.stage_prefetched_page(
                read,
                job,
                prefetch,
                &page,
                updated_page,
                updated_tracker,
                free_history,
                checkpoint,
            )?;
        }
        job["progress"] = json!({"stage":"collecting","pages":checkpoint.pages,"detailsCompleted":checkpoint.details_completed,"requestsCompleted":checkpoint.pages+checkpoint.details_completed,"products":checkpoint.product_ids.len(),"published":prefetch.and_then(|p|p.early.lock().ok().map(|e|e.published.len())).unwrap_or(checkpoint.published_products.len()),"source":source,"metadataCompleted":checkpoint.product_ids.len()-checkpoint.enrichment_pending.len(),"metadataTotal":checkpoint.product_ids.len()});
        if let Some(prefetch) = prefetch {
            let early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            set_collecting_totals(job, &early, source);
        }
        self.progress(app, job)?;
        Ok(())
    }

    /// Queues public metadata for the page's newly listed products and publishes the
    /// rows it discovered while later pages are still read.
    fn stage_prefetched_page(
        &self,
        read: SourceRead<'_>,
        job: &mut Value,
        prefetch: &PublicPrefetch,
        page: &parser::Page,
        updated_page: bool,
        updated_tracker: &Option<UpdatedTracker>,
        free_history: &BTreeMap<String, Vec<parser::Row>>,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        let SourceRead {
            app,
            account,
            source,
            mode,
            key,
            ..
        } = read;
        let mut discovered = page.clone();
        if updated_page {
            // A file's page stays one the list always has.
            discovered.route = format!("{}?page=1", safety::source_path(source)?);
            discovered.rows.retain(|row| {
                updated_tracker
                    .as_ref()
                    .is_some_and(|updated| updated.changes(row))
            });
        }
        if let Some(evidence) = &page.free_item {
            discovered.rows = free_history
                .get(&evidence.product_id)
                .cloned()
                .unwrap_or_default();
            for row in &mut discovered.rows {
                augment_free_row(row, evidence);
            }
        }
        // Free history queues metadata after its item endpoint so
        // that support HTML is already available to reuse. Other
        // sources keep publishing while these endpoints are pending.
        let products: BTreeSet<_> = discovered
            .rows
            .iter()
            .filter(|_| source != "free" || page.free_item.is_some())
            .filter_map(|row| row.product_id.as_ref())
            .filter(|id| checkpoint.enrichment_pending.contains(*id))
            .collect();
        for product in products {
            let first = prefetch.enqueue_once(product)?;
            if !first {
                continue;
            }
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            for row in discovered
                .rows
                .iter()
                .filter(|r| r.product_id.as_ref() == Some(product))
            {
                self.plan_row(
                    &mut batch,
                    account,
                    source,
                    row,
                    &page.route,
                    mode != "incremental",
                )?;
            }
            if let Some(draft) = batch.get("product", product)? {
                prefetch
                    .requests
                    .send(PublicRequest {
                        product: product.clone(),
                        draft,
                        account: account.into(),
                        source: source.into(),
                        run_id: checkpoint.run_id.clone(),
                    })
                    .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_FAILED"))?;
            }
        }
        let published = self.stage_discovered_page(
            job,
            account,
            source,
            mode,
            key,
            checkpoint,
            &discovered,
            prefetch,
        )?;
        source_changed(app, published);
        Ok(())
    }

    /// The checkpoint this pass continues, or a fresh one when the list moved under it.
    async fn resume_checkpoint(
        &self,
        key: &str,
        account: &str,
        source: &str,
        mode: &str,
        client: &reqwest::Client,
        selected: Option<&BTreeSet<String>>,
        prefetch: Option<&PublicPrefetch>,
    ) -> Result<Checkpoint> {
        let previous = resumable_checkpoint(
            self.inner.catalogue.get("checkpoint", key)?,
            mode,
            selected.is_some(),
        )?;
        let mut checkpoint = if let Some(previous) = previous.filter(|p| {
            !p.finished
                && p.version == 1
                && p.publication_version == 1
                && p.account_id == account
                && p.source == source
                && p.mode == mode
                && mode != "forced"
                && selected.is_none()
                && (source != "free" || p.free_item_adapter_version == FREE_ITEM_ADAPTER_VERSION)
        }) {
            previous
        } else {
            Checkpoint::fresh(account, source, mode)?
        };
        // An interrupted source rereads both overlap boundaries before continuing its queue.
        if !checkpoint.committed.is_empty() {
            let head_route = format!("{}?page=1", safety::source_path(source)?);
            let head = self
                .fetch_page(client, source, &head_route, prefetch)
                .await?;
            let mut boundary_page = None;
            if let Some(boundary) = checkpoint.boundary.as_deref().filter(|b| *b != head_route) {
                safety::route(boundary, source)?;
                boundary_page = match self.fetch_page(client, source, boundary, prefetch).await {
                    // Moved, as far as this run can tell: the fresh pass meets the refusal itself.
                    Err(error) if refused_updated_order(boundary, &error) => None,
                    page => Some(page?),
                };
            }
            let moved = overlap_changed(&checkpoint, &head, boundary_page.as_ref());
            if moved {
                checkpoint = Checkpoint::fresh(account, source, mode)?;
            }
        }
        Ok(checkpoint)
    }

    /// Rereads the pass's journaled pages after an interruption: the free history they
    /// listed, and where the trackers stood.
    fn replay_journals(
        &self,
        checkpoint: &Checkpoint,
        account: &str,
        source: &str,
        baseline: Option<&Baseline>,
        tracker: &mut Option<Tracker>,
        updated_tracker: &mut Option<UpdatedTracker>,
    ) -> Result<BTreeMap<String, Vec<parser::Row>>> {
        // Reconstruct exactly the durable parsed prefix after a cancellation/crash.
        let mut recovered_overlap = None;
        let mut free_history = BTreeMap::new();
        for id in &checkpoint.journal_ids {
            let value = self
                .inner
                .catalogue
                .get("checkpoint", id)?
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
            let page = checked_journal_page(&value, account, source, &checkpoint.run_id)?;
            remember_free_history(&page, &mut free_history);
            if page
                .free_item
                .as_ref()
                .is_some_and(|e| !free_history.contains_key(&e.product_id))
            {
                bail!("FREE_ITEM_HISTORY_UNPROVEN");
            }
            if safety::updated_order(&page.route) {
                if let Some(updated) = updated_tracker
                    .as_mut()
                    .filter(|_| recovered_overlap.is_some())
                {
                    updated.observe(&page);
                }
            } else if let Some(tracker) = tracker.as_mut().filter(|_| recovered_overlap.is_none()) {
                let proof = tracker.observe(&page);
                if checkpoint.overlap_end.is_some() && proof == checkpoint.overlap_end {
                    recovered_overlap = proof;
                    *updated_tracker = baseline.map(|base| tracker.updated(&base.updated_head));
                }
            }
        }
        if checkpoint.overlap_end.is_some() && checkpoint.overlap_end != recovered_overlap {
            bail!("CHECKPOINT_INVALID");
        }
        Ok(free_history)
    }
    fn commit_source_page(
        &self,
        journal_id: &str,
        journal: &Value,
        key: &str,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        let mut durable = serde_json::to_value(checkpoint)?;
        durable["id"] = json!(key);
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            batch.stage("checkpoint", journal_id, journal.clone())?;
            batch.stage("checkpoint", key, durable.clone())?;
            if batch.try_commit()?.is_some() {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    async fn prefetch_metadata(
        &self,
        app: &AppHandle,
        job: &Value,
        early: &std::sync::Arc<Mutex<EarlyPublications>>,
        client: &reqwest::Client,
        mut requests: mpsc::UnboundedReceiver<PublicRequest>,
        results: mpsc::UnboundedSender<(String, Result<Value, String>)>,
        support_pages: &Arc<Mutex<SupportPages>>,
    ) -> Result<()> {
        let observe = move |request: PublicRequest| async move {
            let id = request.product.clone();
            let draft_id = request.draft_id();
            let mut draft = request.draft.clone();
            let saved = self
                .inner
                .catalogue
                .get("checkpoint", &draft_id)?
                .and_then(|saved| request.saved_product(&saved));
            let outcome = if let Some(saved) = saved {
                draft = saved;
                Ok(())
            } else {
                self.enrich_into(client, &id, &mut draft, Some(support_pages))
                    .await
            };
            let result = outcome
                .map(|_| public_patch(&request.draft, &draft))
                .map_err(|error| super::code(&error));
            // Current-run parsed product draft only; never publish it here.
            // Completed public requests survive cancellation/restart.
            self.save_product_draft(
                &draft_id,
                json!({"id":draft_id,"accountId":request.account,"source":request.source,
                    "runId":request.run_id,"productId":id,"product":draft,
                    "enriched":result.is_ok(),"errorCode":result.as_ref().err()}),
            )?;
            // Exhausted server throttling stops the whole pipeline;
            // do not advance immediately to another product request.
            if result
                .as_ref()
                .err()
                .is_some_and(|code| code == "RATE_LIMITED")
            {
                bail!("RATE_LIMITED");
            }
            Ok::<_, anyhow::Error>((request, result))
        };
        let mut seen = BTreeSet::new();
        let mut workers = FuturesUnordered::new();
        let mut retry = PublicRetry::default();
        let mut closed = false;
        loop {
            if closed && workers.is_empty() {
                if retry.deferred.is_empty() || results.is_closed() {
                    return Ok(());
                }
                if !retry.retrying {
                    // Let a dropped connection or brief outage recover first.
                    for _ in 0..30 {
                        if self.cancelled(job)? {
                            bail!("CANCELLED");
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
                if let Some(request) = retry.next() {
                    workers.push(observe(request));
                }
                continue;
            }
            tokio::select! {
                request = requests.recv(), if !closed && workers.len() < 2 => {
                    match request {
                        Some(request) if seen.insert(request.product.clone()) => {
                            if seen.len() > 10000 { bail!("SOURCE_TRAVERSAL_LIMIT"); }
                            workers.push(observe(request));
                        }
                        Some(_) => {},
                        None => closed = true,
                    }
                }
                result = workers.next(), if !workers.is_empty() => {
                    if let Some(result) = result {
                        let (request, result) = result?;
                        for (product, result) in retry.settle(request, result) {
                            {
                                let mut early = early.lock().map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
                                early.observations.insert(product.clone(), result.clone());
                                self.publish_early(app,job,&mut early,Some(&BTreeSet::from([product.clone()])))?;
                            }
                            if results.send((product, result)).is_err() { return Ok(()); }
                        }
                    }
                }
            }
        }
    }
    fn publish_early(
        &self,
        app: &AppHandle,
        job: &Value,
        early: &mut EarlyPublications,
        candidates: Option<&BTreeSet<String>>,
    ) -> Result<()> {
        let products = self.publish_early_records(job, early, candidates)?;
        source_changed(app, products);
        Ok(())
    }
    // Each complete, bound grant can be consumed before the next listing page.
    // No source-wide closure is claimed by these additive observations.
    fn stage_discovered_page(
        &self,
        job: &Value,
        account: &str,
        source: &str,
        mode: &str,
        key: &str,
        checkpoint: &Checkpoint,
        page: &parser::Page,
        prefetch: &PublicPrefetch,
    ) -> Result<Vec<String>> {
        let mut early = prefetch
            .early
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        early
            .publications
            .checkpoints
            .insert(key.into(), checkpoint.clone());
        let products: BTreeSet<_> = page
            .rows
            .iter()
            .filter(|row| !row.files.is_empty())
            .filter_map(|row| row.product_id.clone())
            .collect();
        for product in &products {
            let incomplete = page.rows.iter().any(|row| {
                row.product_id.as_deref() == Some(product.as_str())
                    && (!row.manifest_complete || row.error_code.is_some())
            });
            if incomplete {
                if source == "free"
                    && (page.free_item.is_none()
                        || page.free_item.as_ref().is_some_and(|evidence| {
                            settled_free_absence(
                                page,
                                &BTreeMap::from([(evidence.product_id.clone(), page.rows.clone())]),
                            )
                        }))
                {
                    // History awaits its product endpoint; a confirmed absent
                    // offer cannot block a different source's complete grant.
                } else {
                    early.publications.incomplete_scopes.insert((
                        account.into(),
                        source.into(),
                        product.clone(),
                    ));
                }
            }
            let incoming: Vec<_> = page
                .rows
                .iter()
                .filter(|row| {
                    row.product_id.as_deref() == Some(product.as_str())
                        && !row.files.is_empty()
                        && row.manifest_complete
                        && row.error_code.is_none()
                })
                .collect();
            if incoming.is_empty() {
                continue;
            }
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            for row in &incoming {
                self.plan_row(
                    &mut batch,
                    account,
                    source,
                    row,
                    &page.route,
                    mode != "incremental",
                )?;
            }
            let draft = batch
                .get("product", product)?
                .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?;
            let entries = early
                .publications
                .products
                .entry(product.clone())
                .or_default();
            if !entries
                .iter()
                .any(|entry| entry.source == source && entry.account == account)
            {
                entries.push(ProductPublication {
                    account: account.into(),
                    source: source.into(),
                    product: product.clone(),
                    rows: vec![],
                    routes: BTreeMap::new(),
                    draft: draft.clone(),
                    files: BTreeSet::new(),
                    checkpoint_key: key.into(),
                    force_verify: mode != "incremental",
                    needs_enrichment: checkpoint.enrichment_pending.contains(product),
                });
            }
            let entry = entries
                .iter_mut()
                .find(|entry| entry.source == source && entry.account == account)
                .unwrap();
            entry.draft = draft;
            for row in incoming {
                entry
                    .rows
                    .retain(|previous| previous.source_record_id != row.source_record_id);
                entry.rows.push(row.clone());
                entry
                    .routes
                    .insert(row.source_record_id.clone(), page.route.clone());
            }
            entry.files = observed_file_ids(account, entry.rows.iter());
        }
        self.publish_early_records(job, &mut early, Some(&products))
    }
    fn publish_early_records(
        &self,
        job: &Value,
        early: &mut EarlyPublications,
        candidates: Option<&BTreeSet<String>>,
    ) -> Result<Vec<String>> {
        let mut published = Vec::new();
        if early.closed {
            return Ok(published);
        }
        // Most callbacks change one product or one listing page. Do not visit
        // or serialize every earlier draft to discover it is unchanged.
        let candidates = candidates
            .cloned()
            .unwrap_or_else(|| early.publications.products.keys().cloned().collect());
        let candidates: Vec<_> = candidates
            .into_iter()
            .filter(|product| {
                // Library-only observations cannot supply an unseen receipt's
                // date/paid fields. A complete receipt/gift/free grant can.
                early
                    .publications
                    .products
                    .get(product)
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry.source != "library"
                                || early.remaining_sources.is_empty()
                                    && publication_membership_proven(
                                        entries,
                                        &early.publications.unbound_sources,
                                    )
                        })
                    })
            })
            .collect();
        for product in candidates {
            if early.publications.products[&product]
                .iter()
                .all(|entry| publication_entry_published(entry, &early.publications.checkpoints))
            {
                // Restored scopes supply membership/progress context but have
                // no new facts to replay in the additive discovery pass.
                early.published.insert(product);
                continue;
            }
            if let Some(Ok(patch)) = early.observations.get(&product) {
                for entry in early.publications.products.get_mut(&product).unwrap() {
                    if publication_entry_published(entry, &early.publications.checkpoints) {
                        continue;
                    }
                    entry
                        .draft
                        .as_object_mut()
                        .unwrap()
                        .extend(patch.as_object().unwrap().clone());
                    entry.needs_enrichment = false;
                }
            }
            let entries = &early.publications.products[&product];
            let ready: Vec<_> = entries
                .iter()
                .filter(|entry| {
                    !publication_entry_published(entry, &early.publications.checkpoints)
                        && (!entry.needs_enrichment || publication_entry_has_complete_grant(entry))
                })
                .cloned()
                .collect();
            if !ready.is_empty()
                && (ready.iter().any(|entry| entry.source != "library")
                    || early.remaining_sources.is_empty()
                        && publication_membership_proven(
                            entries,
                            &early.publications.unbound_sources,
                        ))
            {
                let version = additive_version(&ready)?;
                if early.publications.additive_versions.get(&product) == Some(&version) {
                    continue;
                }
                let changed = self.commit_product_group_mode(
                    job,
                    &product,
                    &ready,
                    &mut early.publications.checkpoints,
                    false,
                )?;
                early
                    .publications
                    .additive_versions
                    .insert(product.clone(), version);
                early.published.insert(product.clone());
                // Unchanged products need no list refresh while the sync runs.
                if changed {
                    published.push(product);
                }
            }
        }
        Ok(published)
    }
    async fn publish_pipeline(
        &self,
        app: &AppHandle,
        job: &mut Value,
        publications: &mut Publications,
        mut results: mpsc::UnboundedReceiver<(String, Result<Value, String>)>,
    ) -> Result<BTreeSet<String>> {
        let products = publication_products(publications);
        let total = products.len();
        let mut pending: BTreeSet<_> = products
            .iter()
            .filter(|id| publication_entries(publications, id).any(|e| e.needs_enrichment))
            .cloned()
            .collect();
        let mut public_failed = false;
        let mut refused = BTreeSet::new();
        let mut completed = total - pending.len();
        let mut published = published_product_count(publications);
        // This must also run when every prefetched observation is already done.
        // Source jobs are clones; the original job has only enqueue counters.
        job["progress"] = publication_progress(publications, "enriching", completed, published);
        self.progress(app, job)?;
        // Products with retained valid public metadata need no new request.
        for (index, product) in products.iter().enumerate() {
            if index % 16 == 0 {
                // Certification still shares the public-prefetch task. Give
                // pending HTTP futures a turn before each bounded group.
                tokio::task::yield_now().await;
                if self.cancelled(job)? {
                    bail!("CANCELLED");
                }
            }
            if self.publish_ready_product(app, job, publications, product)? {
                published += 1;
            }
        }
        job["progress"] = publication_progress(publications, "enriching", completed, published);
        self.progress(app, job)?;
        while !pending.is_empty() {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            let (product, observation) = results
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_FAILED"))?;
            if !pending.remove(&product) {
                continue;
            }
            completed += 1;
            match observation {
                Ok(patch) => {
                    for (availability, entry) in publications
                        .products
                        .get_mut(&product)
                        .into_iter()
                        .flatten()
                        .map(|entry| (false, entry))
                        .chain(
                            publications
                                .availability
                                .get_mut(&product)
                                .into_iter()
                                .flatten()
                                .map(|entry| (true, entry)),
                        )
                    {
                        if !availability
                            && publication_entry_published(entry, &publications.checkpoints)
                        {
                            continue;
                        }
                        {
                            entry
                                .draft
                                .as_object_mut()
                                .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?
                                .extend(patch.as_object().unwrap().clone());
                            entry.needs_enrichment = false;
                            let checkpoint = &publications.checkpoints[&entry.checkpoint_key];
                            let draft_id = format!("booth-product:{}:{product}", checkpoint.run_id);
                            self.save_product_draft(&draft_id,
                                json!({"id":draft_id,"accountId":entry.account,"source":entry.source,"runId":checkpoint.run_id,"productId":product,"product":entry.draft,"enriched":true}))?;
                        }
                    }
                }
                Err(code) => {
                    // A refused or unreadable public page keeps that product's metadata
                    // as it was without holding up the job; an outage still does.
                    if product_local_failure(&code) {
                        refused.insert(product.clone());
                    } else {
                        public_failed = true;
                    }
                    let keys: Vec<_> = publication_entries(publications, &product)
                        .map(|entry| entry.checkpoint_key.clone())
                        .collect();
                    for key in keys {
                        publications
                            .checkpoints
                            .get_mut(&key)
                            .unwrap()
                            .enrichment_pending
                            .insert(product.clone());
                    }
                }
            }
            if self.publish_ready_product(app, job, publications, &product)? {
                published += 1;
            }
            job["progress"] = publication_progress(publications, "enriching", completed, published);
            self.progress(app, job)?;
        }
        self.publish_collected(app, job, publications).await?;
        // That product's public fields stay pending for the next sync; one
        // failed public page does not turn committed account work into a
        // whole-sync network failure.
        if public_failed {
            bail!("PUBLIC_METADATA_FAILED");
        }
        Ok(refused)
    }
    fn publish_ready_product(
        &self,
        app: &AppHandle,
        job: &mut Value,
        publications: &mut Publications,
        product: &str,
    ) -> Result<bool> {
        if product_publication_done(publications, product) {
            return Ok(false);
        }
        if let Some(entries) = publications.availability.get(product) {
            if !publications.availability_published.contains(product)
                && entries.iter().all(|entry| !entry.needs_enrichment)
                && entries.iter().all(|entry| {
                    !publications
                        .unbound_sources
                        .contains(&(entry.account.clone(), entry.source.clone()))
                })
            {
                self.commit_availability_product(job, product, entries, &publications.checkpoints)?;
                for entry in entries {
                    publications
                        .checkpoints
                        .get_mut(&entry.checkpoint_key)
                        .unwrap()
                        .enrichment_pending
                        .remove(product);
                }
                publications.availability_published.insert(product.into());
                source_changed(app, std::iter::once(product.into()));
            }
        }
        if !publications.products.contains_key(product) {
            return Ok(product_publication_done(publications, product));
        }
        let (authoritative, additive) = ready_product_entries(publications, product);
        if authoritative.is_empty() && additive.is_empty() {
            return Ok(false);
        }
        job["progress"]["stage"] = json!("publishing");
        // Each product does not need its own durable job write; callers save
        // the counters after the loop and check cancellation in batches.
        if self.progress_due(job)? {
            self.progress(app, job)?;
        }
        if !additive.is_empty() {
            let version = additive_version(&additive)?;
            if publications.additive_versions.get(product) != Some(&version) {
                let changed = self.commit_product_group_mode(
                    job,
                    product,
                    &additive,
                    &mut publications.checkpoints,
                    false,
                )?;
                publications
                    .additive_versions
                    .insert(product.into(), version);
                if changed {
                    source_changed(app, std::iter::once(product.into()));
                }
            }
        }
        if !authoritative.is_empty()
            && self.commit_product_group(
                job,
                product,
                &authoritative,
                &mut publications.checkpoints,
            )?
        {
            source_changed(app, std::iter::once(product.into()));
        }
        Ok(product_publication_done(publications, product))
    }
    async fn publish_collected(
        &self,
        app: &AppHandle,
        job: &mut Value,
        publications: &mut Publications,
    ) -> Result<()> {
        let total = publication_products(publications).len();
        let mut published = published_product_count(publications);
        for (index, product) in publication_products(publications).iter().enumerate() {
            if index % 16 == 0 {
                tokio::task::yield_now().await;
                if self.cancelled(job)? {
                    bail!("CANCELLED");
                }
            }
            if product_publication_done(publications, product) {
                continue;
            }
            job["progress"]["stage"] = json!("publishing");
            job["progress"]["metadataTotal"] = json!(total);
            job["progress"]["metadataCompleted"] = json!(total);
            job["progress"]["published"] = json!(published);
            self.progress(app, job)?;
            if self.publish_ready_product(app, job, publications, product)? {
                published += 1;
            }
        }
        let mut final_records = Vec::new();
        for (kind, id, mut value) in std::mem::take(&mut publications.final_records) {
            let source_checkpoint = publications.checkpoints.values().find(|checkpoint| {
                value["accountId"] == checkpoint.account_id && value["source"] == checkpoint.source
            });
            let complete = source_checkpoint.is_some_and(|checkpoint| {
                checkpoint.complete()
                    && !publications
                        .unbound_sources
                        .contains(&(checkpoint.account_id.clone(), checkpoint.source.clone()))
                    && !source_has_incomplete_product(checkpoint, &publications.incomplete_scopes)
            });
            if kind == "checkpoint" && id.starts_with("booth-complete:") && !complete {
                continue;
            }
            if kind == "source_coverage" {
                value["status"] = json!(if complete { "complete" } else { "incomplete" });
                value["coverageComplete"] = json!(complete);
                value["errorCode"] = if complete {
                    Value::Null
                } else {
                    json!("SYNC_INCOMPLETE")
                };
            }
            if kind == "checkpoint" {
                if let Some(checkpoint) = publications.checkpoints.get(&id) {
                    let mut checkpoint = checkpoint.clone();
                    // Retry completed discovery with pending public work; a
                    // genuinely unresolved source must be traversed afresh.
                    checkpoint.finished = checkpoint_finished_after_collection(
                        &checkpoint,
                        complete,
                        &publications.incomplete_scopes,
                    );
                    value = serde_json::to_value(checkpoint)?;
                    value["id"] = json!(id);
                }
            }
            final_records.push((kind, id, value));
        }
        let completed = publication_products(publications)
            .iter()
            .filter(|product| {
                publication_entries(publications, product).all(|entry| !entry.needs_enrichment)
            })
            .count();
        // Publication can settle enrichment work after source assembly. Refresh
        // the last closure without counting its observed reasons a second time.
        for checkpoint in publications.checkpoints.values() {
            trace_source_closure(checkpoint, publications);
        }
        job["progress"] = publication_progress(publications, "finalizing", completed, published);
        self.progress(app, job)?;
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            let current = batch
                .get("job", field(job, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            if current["state"] == "cancelled" {
                bail!("CANCELLED");
            }
            for (kind, id, value) in &final_records {
                batch.stage(kind, id, value.clone())?;
            }
            let ordered = stage_free_orders(&mut batch, &publications.free_orders)?;
            if batch.try_commit()?.is_some() {
                source_changed(app, ordered);
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    fn commit_product_group(
        &self,
        job: &Value,
        product: &str,
        entries: &[ProductPublication],
        checkpoints: &mut BTreeMap<String, Checkpoint>,
    ) -> Result<bool> {
        self.commit_product_group_mode(job, product, entries, checkpoints, true)
    }
    fn commit_availability_product(
        &self,
        job: &Value,
        product: &str,
        entries: &[ProductPublication],
        checkpoints: &BTreeMap<String, Checkpoint>,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            let current = batch
                .get("job", field(job, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            if current["state"] == "cancelled" {
                bail!("CANCELLED");
            }
            for entry in entries {
                let checkpoint = checkpoints
                    .get(&entry.checkpoint_key)
                    .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
                if entry.product != product
                    || entry.source != "free"
                    || checkpoint.account_id != entry.account
                    || checkpoint.source != "free"
                    || entry.rows.is_empty()
                    || entry.needs_enrichment
                {
                    bail!("CHECKPOINT_INVALID");
                }
                for row in &entry.rows {
                    let route = entry
                        .routes
                        .get(&row.source_record_id)
                        .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
                    if row.product_id.as_deref() != Some(product)
                        || row.unavailable
                        || row.error_code.is_some()
                        || safety::free_item_id(route) != Some(product)
                    {
                        bail!("CHECKPOINT_INVALID");
                    }
                    self.plan_row_mode(
                        &mut batch,
                        &entry.account,
                        "free",
                        row,
                        route,
                        false,
                        true,
                        None,
                    )?;
                }
                let mut draft = batch
                    .get("product", product)?
                    .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?;
                let patch = public_patch(&draft, &entry.draft);
                draft
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?
                    .extend(patch.as_object().unwrap().clone());
                batch.stage("product", product, draft)?;
            }
            if batch.try_commit()?.is_some() {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    fn commit_product_group_mode(
        &self,
        job: &Value,
        product: &str,
        entries: &[ProductPublication],
        checkpoints: &mut BTreeMap<String, Checkpoint>,
        authoritative: bool,
    ) -> Result<bool> {
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            let current = batch
                .get("job", field(job, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            if current["state"] == "cancelled" {
                bail!("CANCELLED");
            }
            let mut updated = BTreeSet::new();
            for entry in entries {
                if entry.product != product {
                    bail!("CHECKPOINT_INVALID");
                }
                if publication_entry_published(entry, checkpoints) {
                    // Keep the scope as evidence for other pending sources;
                    // its certified facts already committed atomically.
                    continue;
                }
                if job["type"] == "boothUpdate" {
                    ensure!(job["args"]["publicOnly"] != true, "ACCOUNT_UNAVAILABLE");
                    ensure!(
                        job["accountId"]
                            .as_str()
                            .is_none_or(|account| account == entry.account),
                        "ACCOUNT_UNAVAILABLE"
                    );
                    ensure!(
                        batch
                            .get("account", &entry.account)?
                            .is_some_and(|account| account["store"] == "BOOTH"),
                        "ACCOUNT_UNAVAILABLE"
                    );
                }
                if authoritative && entry.needs_enrichment {
                    bail!("PUBLIC_METADATA_FAILED");
                }
                let entitlement = format!("{}:{}:{product}", entry.account, entry.source);
                let previous = batch.get("entitlement", &entitlement)?;
                for row in &entry.rows {
                    self.plan_row_mode(
                        &mut batch,
                        &entry.account,
                        &entry.source,
                        row,
                        entry
                            .routes
                            .get(&row.source_record_id)
                            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?,
                        entry.force_verify,
                        false,
                        Some(field(job, "id")?),
                    )?;
                }
                let mut draft = batch
                    .get("product", product)?
                    .unwrap_or_else(|| entry.draft.clone());
                let patch = public_patch(&draft, &entry.draft);
                draft
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?
                    .extend(patch.as_object().unwrap().clone());
                batch.stage("product", product, draft)?;
                let mut variant_ids: BTreeSet<String> = entry
                    .rows
                    .iter()
                    .flat_map(|r| &r.files)
                    .filter_map(|f| f.variant_id.as_deref())
                    .filter(|id| safety::numeric(id))
                    .map(str::to_owned)
                    .collect();
                let mut files = entry.files.clone();
                if !authoritative {
                    if let Some(previous) = &previous {
                        files.extend(
                            previous["fileIds"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                        variant_ids.extend(
                            previous["variantIds"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                    }
                }
                // A complete receipt proves this grant, not that a later order
                // cannot add another owned variant. Retain a previously certified
                // unchanged membership; certify new membership only at closure.
                let unchanged = previous.as_ref().is_some_and(|value| {
                    let saved = |field: &str| -> BTreeSet<String> {
                        value[field]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    };
                    value["manifestComplete"] == true
                        && saved("fileIds") == files
                        && saved("variantIds") == variant_ids
                });
                let exact_free = entry.source == "free"
                    && !entry.rows.is_empty()
                    && entry.rows.iter().all(|row| row.manifest_complete);
                let complete = authoritative || unchanged || exact_free;
                // An unchanged certified membership keeps its basis until closure
                // recertifies it, instead of flipping on every additive pass.
                let basis = if authoritative {
                    "verifiedAccountProductFileList"
                } else if unchanged {
                    previous
                        .as_ref()
                        .and_then(|previous| previous["manifestBasis"].as_str())
                        .unwrap_or("completeObservedGrants")
                } else {
                    "completeObservedGrants"
                };
                let record = Entitlement::certified(
                    entitlement.clone(),
                    &entry.account,
                    &entry.source,
                    product,
                    files,
                    variant_ids,
                    complete,
                    basis,
                );
                records::stage(&mut batch, "entitlement", &entitlement, &record)?;
                if !authoritative {
                    continue;
                }
                let checkpoint = checkpoints
                    .get(&entry.checkpoint_key)
                    .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
                if checkpoint.account_id != entry.account || checkpoint.source != entry.source {
                    bail!("CHECKPOINT_INVALID");
                }
                let draft_id = format!("booth-product:{}:{product}", checkpoint.run_id);
                let signature = publication_manifest(
                    &entry.account,
                    &entry.source,
                    product,
                    &entry.rows,
                    &entry.routes,
                    &entry.files,
                )?;
                // The small current-product marker commits with all published
                // facts and the cancellation guard. The source/page checkpoint
                // is unchanged until finalization, avoiding a whole-source copy
                // for every product. Restart verifies this marker against the
                // durable journals and actual entitlement before skipping work.
                batch.stage(
                    "checkpoint",
                    &draft_id,
                    json!({"id":draft_id,
                    "accountId":entry.account,"source":entry.source,"runId":checkpoint.run_id,
                    "productId":product,"product":entry.draft,"enriched":true,
                    "published":true,"publishedManifest":signature}),
                )?;
                updated.insert(entry.checkpoint_key.clone());
            }
            let changed = batch.changes_facts();
            if batch.try_commit()?.is_some() {
                for key in updated {
                    let checkpoint = checkpoints.get_mut(&key).unwrap();
                    checkpoint.published_products.insert(product.into());
                    checkpoint.enrichment_pending.remove(product);
                }
                return Ok(changed);
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    #[cfg(test)]
    fn publish_product(
        &self,
        job: &Value,
        account: &str,
        source: &str,
        product: &str,
        rows: &[&parser::Row],
        routes: &BTreeMap<String, String>,
        draft: &Value,
        files: &BTreeSet<String>,
        key: &str,
        checkpoint: &mut Checkpoint,
        force_verify: bool,
    ) -> Result<()> {
        let entry = ProductPublication {
            account: account.into(),
            source: source.into(),
            product: product.into(),
            rows: rows.iter().map(|r| (*r).clone()).collect(),
            routes: routes.clone(),
            draft: draft.clone(),
            files: files.clone(),
            checkpoint_key: key.into(),
            force_verify,
            needs_enrichment: false,
        };
        let mut checkpoints = BTreeMap::from([(key.to_owned(), checkpoint.clone())]);
        self.commit_product_group(job, product, &[entry], &mut checkpoints)?;
        *checkpoint = checkpoints.remove(key).unwrap();
        Ok(())
    }
    fn save_checkpoint(&self, key: &str, c: &Checkpoint) -> Result<()> {
        let mut value = serde_json::to_value(c)?;
        value["id"] = json!(key);
        self.inner.catalogue.put("checkpoint", key, value)
    }
    fn save_product_draft(&self, id: &str, value: Value) -> Result<()> {
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            let mut value = value.clone();
            if let Some(saved) = batch.get("checkpoint", id)? {
                if saved["published"] == true
                    && saved["publishedManifest"].is_string()
                    && product_draft_scope(
                        &saved,
                        field(&value, "accountId")?,
                        field(&value, "source")?,
                        field(&value, "runId")?,
                        field(&value, "productId")?,
                    )
                {
                    value["published"] = json!(true);
                    value["publishedManifest"] = saved["publishedManifest"].clone();
                }
            }
            // A late metadata callback cannot erase a concurrently committed
            // publication marker: it retries against the latest draft instead.
            batch.stage("checkpoint", id, value)?;
            if batch.try_commit()?.is_some() {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    fn restore_product_publication(
        &self,
        product: &str,
        rows: &[&parser::Row],
        routes: &BTreeMap<String, String>,
        files: &BTreeSet<String>,
        manifest_complete: bool,
        saved: Option<&Value>,
        checkpoint: &mut Checkpoint,
    ) -> Result<bool> {
        if checkpoint.published_products.contains(product) {
            return Ok(true); // Existing validated current-run checkpoint proof.
        }
        let Some(saved) = saved.filter(|saved| {
            manifest_complete
                && saved["published"] == true
                && saved["enriched"] == true
                && product_draft_scope(
                    saved,
                    &checkpoint.account_id,
                    &checkpoint.source,
                    &checkpoint.run_id,
                    product,
                )
        }) else {
            return Ok(false);
        };
        let signature = publication_manifest(
            &checkpoint.account_id,
            &checkpoint.source,
            product,
            rows.iter().copied(),
            routes,
            files,
        )?;
        if saved["publishedManifest"] != signature {
            return Ok(false);
        }
        let entitlement_id = format!("{}:{}:{product}", checkpoint.account_id, checkpoint.source);
        let Some(entitlement) = self.inner.catalogue.get("entitlement", &entitlement_id)? else {
            return Ok(false);
        };
        let variant_ids: BTreeSet<_> = rows
            .iter()
            .flat_map(|row| &row.files)
            .filter_map(|file| file.variant_id.as_deref())
            .filter(|id| safety::numeric(id))
            .map(str::to_owned)
            .collect();
        if entitlement["accountId"] != checkpoint.account_id
            || entitlement["source"] != checkpoint.source
            || entitlement["productId"] != product
            || entitlement["manifestComplete"] != true
            || entitlement["currentManifest"] != true
            || entitlement["manifestBasis"] != "verifiedAccountProductFileList"
            || entitlement["fileIds"] != json!(files)
            || entitlement["variantIds"] != json!(variant_ids)
            || self.inner.catalogue.get("product", product)?.is_none()
        {
            return Ok(false);
        }
        checkpoint.published_products.insert(product.into());
        checkpoint.enrichment_pending.remove(product);
        Ok(true)
    }
    fn retain_published_context(
        &self,
        publications: &mut Publications,
        prefetch: Option<&PublicPrefetch>,
        entry: ProductPublication,
    ) -> Result<()> {
        if let Some(prefetch) = prefetch {
            let mut early = prefetch
                .early
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            let checkpoint = early
                .publications
                .checkpoints
                .get_mut(&entry.checkpoint_key)
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
            checkpoint.published_products.insert(entry.product.clone());
            checkpoint.enrichment_pending.remove(&entry.product);
            early.published.insert(entry.product.clone());
            remember_product_context(&mut early.publications, entry.clone());
        }
        remember_product_context(publications, entry);
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn record_row(
        &self,
        account: &str,
        source: &str,
        row: &parser::Row,
        route: &str,
        force_verify: bool,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut batch = super::batch::RecordBatch::new(&self.inner.catalogue);
            self.plan_row(&mut batch, account, source, row, route, force_verify)?;
            if batch.try_commit()?.is_some() {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    pub(super) fn plan_row(
        &self,
        batch: &mut super::batch::RecordBatch<'_>,
        account: &str,
        source: &str,
        row: &parser::Row,
        route: &str,
        force_verify: bool,
    ) -> Result<()> {
        self.plan_row_mode(
            batch,
            account,
            source,
            row,
            route,
            force_verify,
            false,
            None,
        )
    }
    fn plan_row_mode(
        &self,
        batch: &mut super::batch::RecordBatch<'_>,
        account: &str,
        source: &str,
        row: &parser::Row,
        route: &str,
        force_verify: bool,
        history_only: bool,
        observation_job: Option<&str>,
    ) -> Result<()> {
        let canonical_row = if batch
            .get(
                "acquisition",
                &format!("{account}:{}", row.source_record_id),
            )?
            .is_none()
            && parser::product_field_file_keys(row).is_some()
        {
            let previous = batch.related("acquisition", row.product_id.as_deref().unwrap())?;
            unique_previous_acquisition_record(&previous, account, source, row).map(
                |source_record_id| {
                    let mut canonical = row.clone();
                    canonical.source_record_id = source_record_id;
                    canonical
                },
            )
        } else {
            None
        };
        let row = canonical_row.as_ref().unwrap_or(row);
        Self::stage_acquisition(batch, account, source, row, route)?;
        let Some(product_id) = &row.product_id else {
            return Ok(());
        };
        if row.files.is_empty() && source == "purchase" {
            return Ok(());
        }
        let mut product=batch.get("product",product_id)?.unwrap_or_else(||json!({"id":product_id,"store":"BOOTH","title":row.name,"sourceTitle":row.name,"name":row.name,"author":row.shop,"shop":row.shop,"sourceShop":{"name":row.shop,"url":row.shop_url},"previewUrl":row.preview_url,"previews":row.preview_url.iter().collect::<Vec<_>>(),"sourceUrl":format!("https://booth.pm/ja/items/{product_id}"),"sourceTags":[],"tags":[],"sourceCategory":null}));
        // Sparse account observations must not replace richer last reliable public metadata.
        for (key, value) in [
            ("title", row.name.as_ref()),
            ("sourceTitle", row.name.as_ref()),
            ("name", row.name.as_ref()),
            ("shop", row.shop.as_ref()),
            ("author", row.shop.as_ref()),
            ("previewUrl", row.preview_url.as_ref()),
        ] {
            if product[key].is_null() {
                if let Some(value) = value {
                    product[key] = json!(value);
                }
            }
        }
        product["observedAt"] = json!(now());
        metadata::ensure_source_fields(&mut product)?;
        batch.stage("product", product_id, product)?;
        if history_only {
            return Ok(());
        }
        let entitlement_id = format!("{account}:{source}:{product_id}");
        let mut file_ids = vec![];
        for f in &row.files {
            let file_id = format!(
                "{account}:{}:{}",
                f.id,
                f.variant_id.as_deref().unwrap_or("")
            );
            file_ids.push(file_id.clone());
            let existing = batch.get("remote_file", &file_id)?;
            let observation = observation_job
                .map(|job| source_file_observation(job, account, product_id, f))
                .transpose()?;
            // A later grant or closing certification can replay a descriptor
            // already refreshed and positively verified during this same job.
            // New jobs and changed source facts must still force verification.
            let already_observed = observation.as_ref().is_some_and(|token| {
                existing.as_ref().is_some_and(|saved| {
                    saved["sourceObservationToken"].as_str() == Some(token.as_str())
                        && saved["accountId"] == account
                        && saved["productId"] == product_id.as_str()
                })
            });
            let revision =
                revision_observation(existing.as_ref(), f, force_verify && !already_observed);
            let remote = RemoteFile::observe(
                file_id.clone(),
                account,
                product_id,
                source,
                row,
                f,
                route,
                existing.as_ref(),
                revision,
                observation,
            )?;
            records::stage(batch, "remote_file", &file_id, &remote)?;
        }
        let existing = batch.get("entitlement", &entitlement_id)?;
        let entitlement = Entitlement::listed(
            entitlement_id.clone(),
            account,
            source,
            product_id,
            file_ids,
            row.unavailable,
            existing.as_ref(),
        );
        records::stage(batch, "entitlement", &entitlement_id, &entitlement)
    }

    /// The row's acquisition record, keeping what a sparser listing no longer shows.
    fn stage_acquisition(
        batch: &mut super::batch::RecordBatch<'_>,
        account: &str,
        source: &str,
        row: &parser::Row,
        route: &str,
    ) -> Result<()> {
        let id = format!("{account}:{}", row.source_record_id);
        let previous = batch.get("acquisition", &id)?;
        let acquisition =
            Acquisition::observe(id.clone(), account, source, row, route, previous.as_ref());
        records::stage(batch, "acquisition", &id, &acquisition)
    }
    async fn enrich(&self, client: &reqwest::Client, id: &str) -> Result<()> {
        let mut product = self
            .inner
            .catalogue
            .get("product", id)?
            .unwrap_or_else(|| json!({"id":id}));
        self.enrich_into(client, id, &mut product, None).await?;
        self.inner.catalogue.put("product", id, product)
    }
    async fn enrich_into(
        &self,
        client: &reqwest::Client,
        id: &str,
        product: &mut Value,
        support_pages: Option<&Arc<Mutex<SupportPages>>>,
    ) -> Result<()> {
        if !safety::numeric(id) {
            bail!("INVALID_PRODUCT_ID");
        }
        let (status, body) = self
            .inner
            .http
            .get(
                client,
                url::Url::parse(&format!("https://booth.pm/ja/items/{id}.json")).unwrap(),
            )
            .await?;
        if status.as_u16() == 404 || status.as_u16() == 410 {
            metadata::apply_public_response(product, id, status.as_u16(), None)?;
            return Ok(());
        }
        if !status.is_success() {
            bail!(public_status_code(status));
        }
        let value: Value =
            serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
        metadata::apply_public_response(product, id, status.as_u16(), Some(&value))?;
        // The JSON description can omit the dedicated support section. Only
        // supplement products whose digital variants do not establish it.
        if metadata::needs_public_support_html(product) {
            if let Some(support_pages) = support_pages {
                if support_pages
                    .lock()
                    .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                    .apply(id, product)
                {
                    return Ok(());
                }
            }
            let url = url::Url::parse(&format!("https://booth.pm/ja/items/{id}"))?;
            let support = self.inner.http.get(client, url).await;
            match support {
                Ok((status, html)) if status.is_success() => {
                    metadata::apply_public_support_html(product, &html)?
                }
                Ok((status, _)) => {
                    product["publicMetadataState"] = json!("incomplete");
                    product["publicMetadataErrorCode"] = json!(public_status_code(status));
                    bail!(public_status_code(status));
                }
                Err(error) => {
                    // Keep newly obtained price/title fields even when the
                    // required support request needs a later retry.
                    product["publicMetadataState"] = json!("incomplete");
                    product["publicMetadataErrorCode"] = json!(super::code(&error));
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}
fn same_acquisition_binding(
    previous: &Value,
    account: &str,
    source: &str,
    row: &parser::Row,
) -> bool {
    if row.source_record_id.is_empty()
        || previous["accountId"] != account
        || previous["source"] != source
        || previous["sourceRecordId"] != row.source_record_id
    {
        return false;
    }
    let known = |value: Option<&str>| {
        value
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    let orders: BTreeSet<_> = [
        known(previous["orderId"].as_str()),
        known(row.order_id.as_deref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let products: BTreeSet<_> = [
        known(previous["productId"].as_str()),
        known(previous["observedProductId"].as_str()),
        known(row.product_id.as_deref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    orders.len() <= 1 && products.len() <= 1
}
fn unique_previous_acquisition_record(
    previous: &[Value],
    account: &str,
    source: &str,
    row: &parser::Row,
) -> Option<String> {
    let product = row.product_id.as_deref().filter(|id| safety::numeric(id))?;
    let order = row.order_id.as_deref().unwrap_or("");
    if source == "purchase" && !safety::numeric(order) {
        return None;
    }
    if !matches!(source, "purchase" | "library" | "gift" | "free") {
        return None;
    }
    let valid_date = |date: &str| {
        date.is_empty()
            || parser::slash_date_shape(date)
            || chrono::DateTime::parse_from_rfc3339(date).is_ok()
            || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
    };
    let fresh_date = row.acquired_date_text.as_deref().unwrap_or("");
    if !valid_date(fresh_date) {
        return None;
    }
    let record_id = |variant: &str, date: &str| {
        format!(
            "{source}:{}",
            safety::hash(format!("{order}:{product}:{variant}:{date}:"))
        )
    };
    let mut variants = BTreeSet::from([String::new()]);
    variants.extend(row.files.iter().filter_map(|file| file.variant_id.clone()));
    // Explicit line IDs and occurrence suffixes are not date-based identities.
    let bound_variants: Vec<_> = variants
        .into_iter()
        .filter(|variant| {
            row.source_record_id == record_id(variant, fresh_date)
                || row.source_record_id == record_id(variant, "")
        })
        .collect();
    if bound_variants.len() != 1 {
        return None;
    }
    let variant = &bound_variants[0];
    let matching: Vec<_> = previous
        .iter()
        .filter(|saved| {
            if saved["accountId"] != account
                || saved["source"] != source
                || saved["productId"] != product
                || saved["orderId"].as_str().unwrap_or("") != order
                || saved["variantName"].as_str() != row.variant_name.as_deref()
            {
                return false;
            }
            let old_date = saved["acquiredDateText"].as_str().unwrap_or("");
            if !valid_date(old_date)
                || (source != "purchase"
                    && !old_date.is_empty()
                    && !fresh_date.is_empty()
                    && old_date != fresh_date)
            {
                return false;
            }
            let Some(old_record) = saved["sourceRecordId"].as_str() else {
                return false;
            };
            saved["id"] == format!("{account}:{old_record}")
                && (old_record == record_id(variant, old_date)
                    || old_record == record_id(variant, ""))
        })
        .collect();
    // Every same-binding saved native record counts toward uniqueness, including
    // explicit lines/occurrences which cannot be collapsed into a fallback row.
    let bound_count = previous
        .iter()
        .filter(|saved| {
            saved["accountId"] == account
                && saved["source"] == source
                && saved["productId"] == product
                && saved["orderId"].as_str().unwrap_or("") == order
                && saved["variantName"].as_str() == row.variant_name.as_deref()
        })
        .count();
    (matching.len() == 1 && bound_count == 1)
        .then(|| matching[0]["sourceRecordId"].as_str().unwrap().to_owned())
}
fn native_product_line_total(row: &parser::Row, source: &str, route: &str) -> Option<u64> {
    if source != "purchase"
        || row.source_record_id.is_empty()
        || !row.order_id.as_deref().is_some_and(safety::numeric)
        || parser::product_field_file_keys(row).is_none()
    {
        return None;
    }
    let url = safety::route(route, "purchase").ok()?;
    if url.path().strip_prefix("/orders/") != row.order_id.as_deref() || url.query().is_some() {
        return None;
    }
    row.product_line_total_jpy
        .filter(|amount| *amount <= 9_007_199_254_740_991)
}
#[cfg(test)]
fn patched_product_fields(
    existing: &Value,
    account: &str,
    row: &parser::Row,
    route: &str,
) -> Option<Value> {
    let amount = native_product_line_total(row, "purchase", route)?;
    let product = row.product_id.as_deref()?;
    if !same_acquisition_binding(existing, account, "purchase", row)
        || existing["id"] != format!("{account}:{}", row.source_record_id)
        || existing["orderId"].as_str() != row.order_id.as_deref()
        || existing["productId"].as_str() != Some(product)
    {
        return None;
    }
    let mut updated = existing.clone();
    updated["paidPrice"] = json!(amount);
    updated["paidCurrency"] = json!("JPY");
    updated["paidPriceBasis"] = json!("productLineTotal");
    if let Some(date) = row
        .acquired_date_text
        .as_deref()
        .filter(|date| !date.trim().is_empty())
    {
        updated["acquiredDateText"] = json!(date);
    }
    Some(updated)
}
#[cfg(test)]
fn saved_product_line_binding(saved: &[parser::Row], fresh: &parser::Row) -> bool {
    let Some(keys) = parser::diagnostic_digital_file_keys(fresh) else {
        return false;
    };
    let bound: Vec<_> = saved
        .iter()
        .filter(|row| row.source_record_id == fresh.source_record_id)
        .collect();
    bound.len() == 1
        && bound[0].product_id == fresh.product_id
        && bound[0].order_id == fresh.order_id
        && parser::diagnostic_digital_file_keys(bound[0]).as_ref() == Some(&keys)
}
fn source_coverage_record(
    selected: Option<&BTreeSet<String>>,
    id: &str,
    value: Value,
) -> Option<(String, String, Value)> {
    selected
        .is_none()
        .then(|| ("source_coverage".into(), id.into(), value))
}
fn checked_journal_page(
    value: &Value,
    account: &str,
    source: &str,
    run: &str,
) -> Result<parser::Page> {
    if value["accountId"] != account || value["source"] != source || value["runId"] != run {
        bail!("CHECKPOINT_INVALID");
    }
    let page: parser::Page = serde_json::from_value(value["page"].clone())
        .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
    if page.source != source {
        bail!("CHECKPOINT_INVALID");
    }
    if let Some(evidence) = &page.free_item {
        if source != "free"
            || !page.is_detail
            || !page.rows.is_empty()
            || page.terminal
            || !page.next.is_empty()
            || !page.details.is_empty()
            || safety::free_item_id(&page.route) != Some(evidence.product_id.as_str())
            || (evidence.manifest_complete
                && (evidence.files.is_empty()
                    || evidence.unavailable
                    || evidence.error_code.is_some()))
        {
            bail!("CHECKPOINT_INVALID");
        }
    }
    Ok(page)
}
fn collection_settled(
    checkpoint: &Checkpoint,
    complete: bool,
    closed: bool,
    unbound: bool,
    availability_warnings: usize,
) -> bool {
    complete
        || (checkpoint.source == "free"
            && closed
            && !unbound
            && checkpoint.pending.is_empty()
            && checkpoint.terminal
            && !checkpoint.head.is_empty()
            && availability_warnings > 0
            && checkpoint.unresolved == availability_warnings)
}
fn record_source_page(
    checkpoint: &mut Checkpoint,
    source: &str,
    route: &str,
    page: &parser::Page,
    selected: Option<&BTreeSet<String>>,
) -> Result<()> {
    if page.route == format!("{}?page=1", safety::source_path(source)?) {
        checkpoint.head = page.fingerprint.clone();
    }
    if !page.is_detail {
        checkpoint.boundary = Some(page.route.clone());
        // The source's order is that of its plain pages; the updated order only adds to them.
        if !safety::updated_order(&page.route) {
            if checkpoint.pages == 0 {
                checkpoint.ordering = page.ordering;
            } else if checkpoint.ordering != page.ordering {
                checkpoint.ordering = parser::Ordering::Unknown;
            }
        }
    }
    checkpoint
        .committed
        .insert(page.route.clone(), page.fingerprint.clone());
    checkpoint.pending.retain(|p| p != route);
    for next in page.next.iter().chain(page.details.iter()) {
        if safety::free_item_id(next)
            .is_some_and(|id| selected.is_some_and(|ids| !ids.contains(id)))
        {
            continue;
        }
        if !checkpoint.committed.contains_key(next)
            && !checkpoint.pending.contains(next)
            && !checkpoint.reused.contains(next)
        {
            checkpoint.pending.push(next.clone());
        }
    }
    checkpoint.terminal |= page.terminal;
    // A sorted page that cannot be read sends the App to read every page, which covers it.
    if !safety::updated_order(&page.route) {
        checkpoint.unresolved += page.unresolved;
    }
    if page.is_detail {
        checkpoint.details_completed += 1;
    } else {
        checkpoint.pages += 1;
    }
    Ok(())
}
/// Takes listed receipts the previous complete pass already read out of the
/// request queue. Their rows join the source at assembly.
fn reuse_known_receipts(checkpoint: &mut Checkpoint, known: &BTreeMap<String, Vec<parser::Row>>) {
    let mut reused = vec![];
    checkpoint.pending.retain(|route| {
        let reuse = known.contains_key(route);
        if reuse {
            reused.push(route.clone());
        }
        !reuse
    });
    for route in reused {
        checkpoint.product_ids.extend(
            known[&route]
                .iter()
                .filter_map(|row| row.product_id.clone()),
        );
        checkpoint.reused.insert(route);
    }
}
fn collection_membership_complete(
    checkpoint: &Checkpoint,
    incomplete_scopes: &BTreeSet<(String, String, String)>,
) -> bool {
    checkpoint.pending.is_empty()
        && checkpoint.terminal
        && checkpoint.unresolved == 0
        && !source_has_incomplete_product(checkpoint, incomplete_scopes)
}
fn public_patch(_before: &Value, after: &Value) -> Value {
    // Every source draft receives the same complete public observation. A
    // field matching the first draft can still differ on a receipt draft.
    let mut patch: serde_json::Map<String, Value> = after
        .as_object()
        .expect("product object")
        .iter()
        .filter(|(key, _)| metadata::PUBLIC_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if after.get("publicMetadataErrorCode").is_none() {
        patch.insert("publicMetadataErrorCode".into(), Value::Null);
    }
    Value::Object(patch)
}
fn source_file_observation(
    job: &str,
    account: &str,
    product: &str,
    file: &parser::File,
) -> Result<String> {
    Ok(safety::hash(serde_json::to_vec(&(
        job,
        account,
        product,
        &file.id,
        &file.variant_id,
        &file.name,
        file.size_bytes,
        &file.revision,
    ))?))
}
fn revision_observation(
    existing: Option<&Value>,
    file: &parser::File,
    force: bool,
) -> (String, String, bool) {
    if file.revision != "unknown" {
        return (file.revision.clone(), "source".into(), false);
    }
    let unchanged = existing.is_some_and(|e| {
        e["sourceFileId"] == file.id
            && e["variantId"].as_str() == file.variant_id.as_deref()
            && file.name.as_ref().is_none_or(|n| e["name"] == n.as_str())
            && file
                .size_bytes
                .is_none_or(|n| e["sizeBytes"].as_u64().is_none_or(|old| old == n))
    });
    if unchanged {
        if let Some(old) = existing
            .and_then(|e| e["revision"].as_str())
            .filter(|r| !r.is_empty() && *r != "unknown")
        {
            let basis = existing
                .and_then(|e| e["revisionBasis"].as_str())
                .unwrap_or("source")
                .to_owned();
            let pending = existing.is_some_and(|e| e["requiresContentVerification"] == true);
            return (old.to_owned(), basis, force || pending);
        }
    }
    ("unknown".into(), "unknown".into(), true)
}

#[cfg(test)]
mod tests;

/// The Library tracker that lets an incremental pass stop at the rows it already knows.
fn pass_tracker(
    mode: &str,
    selected: Option<&BTreeSet<String>>,
    source: &str,
    checkpoint: &Checkpoint,
    baseline: Option<&Baseline>,
) -> Option<Tracker> {
    // Only the Library is long enough to matter and has a second order to find changes by.
    // Every order and receipt is still read, so a purchase is found whatever this does.
    // Updating chosen assets stops there too: a row they changed leads the updated order,
    // and an unchanged one stays as that pass read it.
    if (mode == "incremental" || selected.is_some())
        && source == "library"
        && !checkpoint.read_every_page
    {
        baseline
            .filter(|b| {
                checkpoint.overlap_end.is_some() || b.recently_read_in_full(chrono::Utc::now())
            })
            .map(Tracker::new)
    } else {
        None
    }
}

/// The pass's rows with a verified tail and reused receipts merged in, the products it read
/// again and whether older observations were kept; every row gets its receipt route.
fn merge_pass_rows(
    checkpoint: &Checkpoint,
    baseline: Option<&Baseline>,
    known_receipts: &BTreeMap<String, Vec<parser::Row>>,
    source: &str,
    head_route: &str,
    current_rows: Vec<parser::Row>,
    updated_rows: &[parser::Row],
    row_routes: &mut BTreeMap<String, String>,
) -> Result<(Vec<parser::Row>, BTreeSet<String>, bool)> {
    let merged = checkpoint
        .overlap_end
        .map(|end| {
            baseline
                .and_then(|base| {
                    super::incremental::merged_rows(base, &current_rows, end, updated_rows)
                })
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))
        })
        .transpose()?;
    let refreshed_products: BTreeSet<_> = current_rows
        .iter()
        .chain(merged.iter().flat_map(|merged| &merged.changed))
        .filter_map(|r| r.product_id.clone())
        .collect();
    let mut all_rows = match merged {
        Some(merged) => merged.rows,
        None => current_rows,
    };
    // Known listed receipts keep their previous complete observation, like a verified tail.
    for route in &checkpoint.reused {
        let rows = known_receipts
            .get(route)
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        all_rows.extend(rows.iter().cloned());
    }
    let retained_observations = checkpoint.overlap_end.is_some() || !checkpoint.reused.is_empty();
    // A verified incremental tail retains its original order identity; its
    // receipt route can be reconstructed without a page-number guess.
    for row in &all_rows {
        row_routes
            .entry(row.source_record_id.clone())
            .or_insert_with(|| {
                row.order_id
                    .as_deref()
                    .filter(|id| source == "purchase" && safety::numeric(id))
                    .map(|id| format!("/orders/{id}"))
                    .unwrap_or_else(|| head_route.to_owned())
            });
    }
    Ok((all_rows, refreshed_products, retained_observations))
}

/// Each listed product's observed files and whether its rows list them in full, and every
/// product's rows in their original order.
fn product_manifests<'a>(
    all_rows: &'a [parser::Row],
    selected: Option<&BTreeSet<String>>,
    account: &str,
    source: &str,
) -> (
    BTreeMap<String, (BTreeSet<String>, bool)>,
    BTreeMap<&'a str, Vec<&'a parser::Row>>,
) {
    // Refreshed prefix rows replace old rows. A changed file list must not be
    // unioned with an obsolete journal row from the previous complete pass.
    let mut manifests: BTreeMap<String, (BTreeSet<String>, bool)> = BTreeMap::new();
    let mut rows_by_product: BTreeMap<&str, Vec<&parser::Row>> = BTreeMap::new();
    for row in all_rows {
        let Some(id) = row.product_id.as_ref() else {
            continue;
        };
        // Preserve every bound receipt row and its original order. Product
        // assembly must not rescan the entire source for each manifest.
        rows_by_product.entry(id.as_str()).or_default().push(row);
        if selected.is_some_and(|s| !s.contains(id))
            || (source == "purchase" && row.files.is_empty())
        {
            continue;
        }
        let manifest = manifests
            .entry(id.clone())
            .or_insert((BTreeSet::new(), true));
        manifest.1 &= row.manifest_complete;
        manifest.0.extend(observed_file_ids(account, [row]));
    }
    (manifests, rows_by_product)
}
