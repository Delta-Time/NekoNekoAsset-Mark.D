//! Which observed grants are complete enough to publish, and publication progress.
use super::*;

#[derive(Default)]
pub(super) struct EarlyPublications {
    pub(super) remaining_sources: BTreeSet<String>,
    pub(super) publications: Publications,
    pub(super) observations: BTreeMap<String, Result<Value, String>>,
    pub(super) published: BTreeSet<String>,
    pub(super) closed: bool,
}

pub(super) fn product_draft_scope(
    saved: &Value,
    account: &str,
    source: &str,
    run: &str,
    product: &str,
) -> bool {
    saved["accountId"] == account
        && saved["source"] == source
        && saved["runId"] == run
        && saved["productId"] == product
        && saved["product"]["id"] == product
}

pub(super) fn publication_manifest<'a>(
    account: &str,
    source: &str,
    product: &str,
    rows: impl IntoIterator<Item = &'a parser::Row>,
    routes: &BTreeMap<String, String>,
    files: &BTreeSet<String>,
) -> Result<String> {
    // Bind publication to these exact journal facts, independent of page order.
    // Public metadata drafts alone do not establish a certified manifest.
    let mut rows: Vec<String> = rows
        .into_iter()
        .map(|row| {
            let route = routes
                .get(&row.source_record_id)
                .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
            Ok(safety::hash(serde_json::to_vec(&(row, route))?))
        })
        .collect::<Result<_>>()?;
    rows.sort_unstable();
    Ok(safety::hash(serde_json::to_vec(&(
        "booth-published-manifest-v1",
        account,
        source,
        product,
        rows,
        files,
    ))?))
}

#[derive(Clone)]
pub(super) struct ProductPublication {
    pub(super) account: String,
    pub(super) source: String,
    pub(super) product: String,
    pub(super) rows: Vec<parser::Row>,
    pub(super) routes: BTreeMap<String, String>,
    pub(super) draft: Value,
    pub(super) files: BTreeSet<String>,
    pub(super) checkpoint_key: String,
    pub(super) force_verify: bool,
    pub(super) needs_enrichment: bool,
}

#[derive(Default)]
pub(super) struct Publications {
    pub(super) products: BTreeMap<String, Vec<ProductPublication>>,
    // Authenticated history remains ownership evidence when the current free
    // offer is unavailable. These facts never certify a file membership.
    pub(super) availability: BTreeMap<String, Vec<ProductPublication>>,
    pub(super) availability_published: BTreeSet<String>,
    pub(super) checkpoints: BTreeMap<String, Checkpoint>,
    pub(super) final_records: Vec<(String, String, Value)>,
    pub(super) incomplete_scopes: BTreeSet<(String, String, String)>,
    pub(super) unbound_sources: BTreeSet<(String, String)>,
    // Exact scan facts retained only until publication refreshes diagnostics.
    // These booleans never enter a source checkpoint or catalogue record.
    pub(super) source_closures: BTreeMap<(String, String), (bool, bool)>,
    pub(super) additive_versions: BTreeMap<String, String>,
}

pub(super) fn source_incomplete_product_count(
    checkpoint: &Checkpoint,
    scopes: &BTreeSet<(String, String, String)>,
) -> u64 {
    scopes
        .iter()
        .filter(|(account, source, _)| {
            account == &checkpoint.account_id && source == &checkpoint.source
        })
        .count() as u64
}

pub(super) fn observed_file_ids<'a>(
    account: &str,
    rows: impl IntoIterator<Item = &'a parser::Row>,
) -> BTreeSet<String> {
    rows.into_iter()
        .flat_map(|row| &row.files)
        .map(|file| {
            format!(
                "{account}:{}:{}",
                file.id,
                file.variant_id.as_deref().unwrap_or("")
            )
        })
        .collect()
}

pub(super) fn observed_grant_rows<'a>(
    source: &str,
    rows: &[&'a parser::Row],
    manifest_complete: bool,
    availability_only: bool,
) -> Vec<&'a parser::Row> {
    rows.iter()
        .copied()
        .filter(|row| {
            if source == "purchase" && row.files.is_empty() {
                return row.error_code.is_none();
            }
            (manifest_complete || availability_only || row.manifest_complete)
                && row.error_code.is_none()
        })
        .collect()
}

pub(super) fn additive_version(entries: &[ProductPublication]) -> Result<String> {
    Ok(safety::hash(serde_json::to_vec(
        &entries
            .iter()
            .map(|entry| {
                (
                    &entry.account,
                    &entry.source,
                    &entry.rows,
                    &entry.routes,
                    &entry.draft,
                    &entry.files,
                )
            })
            .collect::<Vec<_>>(),
    )?))
}

pub(super) fn publication_products(publications: &Publications) -> BTreeSet<String> {
    publications
        .products
        .keys()
        .chain(publications.availability.keys())
        .cloned()
        .collect()
}

pub(super) fn publication_entries<'a>(
    publications: &'a Publications,
    product: &str,
) -> impl Iterator<Item = &'a ProductPublication> {
    publications
        .products
        .get(product)
        .into_iter()
        .flatten()
        .chain(publications.availability.get(product).into_iter().flatten())
}

pub(super) fn product_publication_done(publications: &Publications, product: &str) -> bool {
    publications
        .products
        .get(product)
        .into_iter()
        .flatten()
        .all(|entry| publication_entry_published(entry, &publications.checkpoints))
        && (!publications.availability.contains_key(product)
            || publications.availability_published.contains(product))
}

pub(super) fn publication_entry_published(
    entry: &ProductPublication,
    checkpoints: &BTreeMap<String, Checkpoint>,
) -> bool {
    checkpoints
        .get(&entry.checkpoint_key)
        .is_some_and(|checkpoint| {
            checkpoint.account_id == entry.account
                && checkpoint.source == entry.source
                && checkpoint.published_products.contains(&entry.product)
        })
}

pub(super) fn publication_entry_certifiable(
    entry: &ProductPublication,
    publications: &Publications,
) -> bool {
    let Some(checkpoint) = publications.checkpoints.get(&entry.checkpoint_key) else {
        return false;
    };
    !entry.needs_enrichment
        && checkpoint.account_id == entry.account
        && checkpoint.source == entry.source
        && publication_entry_has_complete_grant(entry)
        && !publications
            .unbound_sources
            .contains(&(entry.account.clone(), entry.source.clone()))
        && (checkpoint.mode == "selected" && entry.checkpoint_key.starts_with("booth-selected:")
            || collection_membership_complete(checkpoint, &publications.incomplete_scopes))
}

pub(super) fn publication_entry_has_complete_grant(entry: &ProductPublication) -> bool {
    let mut observed = false;
    for row in &entry.rows {
        if entry.source == "purchase" && row.files.is_empty() {
            continue;
        }
        observed = true;
        if !row.manifest_complete || row.error_code.is_some() {
            return false;
        }
    }
    observed
}

pub(super) fn ready_product_entries(
    publications: &Publications,
    product: &str,
) -> (Vec<ProductPublication>, Vec<ProductPublication>) {
    publications
        .products
        .get(product)
        .into_iter()
        .flatten()
        .filter(|entry| {
            !publication_entry_published(entry, &publications.checkpoints)
                && publication_entry_has_complete_grant(entry)
        })
        .cloned()
        .partition(|entry| publication_entry_certifiable(entry, publications))
}

pub(super) fn published_product_count(publications: &Publications) -> usize {
    publication_products(publications)
        .iter()
        .filter(|product| product_publication_done(publications, product))
        .count()
}

pub(super) fn remember_product_context(publications: &mut Publications, entry: ProductPublication) {
    let entries = publications
        .products
        .entry(entry.product.clone())
        .or_default();
    entries.retain(|old| old.account != entry.account || old.source != entry.source);
    entries.push(entry);
}

// A free product endpoint is independently scoped to its exact product and
// complete offered file list. It cannot close a different source's unknown row.
pub(super) fn publication_membership_proven(
    entries: &[ProductPublication],
    unbound: &BTreeSet<(String, String)>,
) -> bool {
    unbound.iter().all(|(account, source)| {
        !entries.iter().any(|entry| &entry.account == account)
            || source == "free"
                && entries
                    .iter()
                    .any(|entry| &entry.account == account && entry.source == "free")
    })
}

pub(super) fn publication_progress(
    publications: &Publications,
    stage: &str,
    completed: usize,
    published: usize,
) -> Value {
    let pages: usize = publications.checkpoints.values().map(|c| c.pages).sum();
    let details: usize = publications
        .checkpoints
        .values()
        .map(|c| c.details_completed)
        .sum();
    let total = publication_products(publications).len();
    let availability_warnings: usize = publications.availability.values().map(Vec::len).sum();
    json!({"stage":stage,"source":"all","pages":pages,"detailsCompleted":details,
        "requestsCompleted":pages+details,"products":total,"metadataCompleted":completed,
        "metadataTotal":total,"published":published,"availabilityWarnings":availability_warnings})
}

pub(super) fn set_collecting_totals(job: &mut Value, early: &EarlyPublications, source: &str) {
    let products: BTreeSet<_> = early
        .publications
        .checkpoints
        .values()
        .flat_map(|c| c.product_ids.iter())
        .collect();
    let completed = products
        .iter()
        .filter(|id| early.observations.contains_key(**id))
        .count();
    job["progress"] = publication_progress(
        &early.publications,
        "collecting",
        completed,
        early.published.len(),
    );
    job["progress"]["products"] = json!(products.len());
    job["progress"]["metadataTotal"] = json!(products.len());
    job["progress"]["source"] = json!(source);
}
