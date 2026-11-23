//! Free-history rows remembered between scans.
use super::*;

/// The download history a sync recorded for one free item, as the rows it read then. The
/// history only grows, so these still prove the account has the item.
pub(super) fn saved_free_history(
    acquisitions: &[Value],
    account: &str,
    product: &str,
) -> Vec<parser::Row> {
    acquisitions
        .iter()
        .filter(|a| {
            a["accountId"] == account
                && a["source"] == "free"
                && a["observedProductId"].as_str().or(a["productId"].as_str()) == Some(product)
                && a["unavailable"] != true
                && a["errorCode"].is_null()
        })
        .filter_map(|a| {
            Some(parser::Row {
                source_record_id: a["sourceRecordId"]
                    .as_str()
                    .filter(|id| !id.is_empty())?
                    .into(),
                product_id: Some(product.into()),
                acquired_date_text: a["acquiredDateText"].as_str().map(str::to_owned),
                variant_name: a["variantName"].as_str().map(str::to_owned),
                ..Default::default()
            })
        })
        .collect()
}

/// The record ID the free history gives an item's row, which names no order, variation or
/// date. A free item added by its listing takes it too, so the history merges with it later.
pub(super) fn free_history_record_id(product: &str) -> String {
    format!("free:{}", safety::hash(format!(":{product}:::")))
}

pub(super) fn eligible_free_rows(page: &parser::Page) -> impl Iterator<Item = &parser::Row> {
    page.rows.iter().filter(|row| {
        page.source == "free"
            && !page.is_detail
            && page.free_item.is_none()
            && safety::route(&page.route, "free").is_ok()
            && !row.source_record_id.is_empty()
            && !row.unavailable
            && row.error_code.is_none()
            && row.product_id.as_deref().is_some_and(safety::numeric)
    })
}

pub(super) fn remember_free_history(
    page: &parser::Page,
    history: &mut BTreeMap<String, Vec<parser::Row>>,
) {
    for row in eligible_free_rows(page) {
        let rows = history.entry(row.product_id.clone().unwrap()).or_default();
        if !rows
            .iter()
            .any(|old| old.source_record_id == row.source_record_id)
        {
            rows.push(row.clone());
        }
    }
}

/// Where a free-history list page puts each row. BOOTH lists the newest download first,
/// page after page, and shows no dates.
pub(super) fn free_history_positions(page: &parser::Page) -> Vec<(String, u64)> {
    let number = (page.source == "free" && !page.is_detail)
        .then(|| safety::route(&page.route, "free").ok())
        .flatten()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "page")
                .and_then(|(_, value)| value.parse::<u64>().ok())
        });
    let Some(number) = number.filter(|number| *number > 0) else {
        return vec![];
    };
    page.rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                row.source_record_id.clone(),
                (number - 1) * 1000 + index as u64,
            )
        })
        .collect()
}

/// What one account's free-history pass listed: when the pass began, and each record's
/// place, 0 being the newest download.
#[derive(Clone, Debug, Default)]
pub(super) struct FreePlaces {
    pub(super) started: String,
    pub(super) records: BTreeMap<String, u64>,
}

impl FreePlaces {
    /// Adds a row's place; a row listed twice keeps its older place.
    pub(super) fn add(&mut self, record: String, place: u64) {
        let kept = self.records.entry(record).or_insert(place);
        *kept = (*kept).max(place);
    }

    pub(super) fn merge(&mut self, other: FreePlaces) {
        if self.started.is_empty() {
            self.started = other.started;
        }
        for (record, place) in other.records {
            self.add(record, place);
        }
    }
}

/// The prefix of the places an account's first pass gives its whole history. It sorts before
/// every pass's start, so accounts synced one after another interleave by place instead of by
/// which was synced first.
const FIRST_PASS: &str = "00000000000000";

/// The prefix a later pass gives the downloads it finds above every placed one: its start.
fn pass_prefix(started_at: &str) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    Some(
        started
            .with_timezone(&chrono::Utc)
            .format("%Y%m%d%H%M%S")
            .to_string(),
    )
}

/// A sort key for an undated free download: a prefix, then BOOTH's newest-first place, so a
/// later pass sorts after an earlier one and within one prefix the place decides. Larger is
/// newer.
pub(super) fn free_history_order(prefix: &str, place: u64) -> Option<String> {
    Some(format!(
        "{prefix}{:012}",
        999_999_999_999u64.checked_sub(place)?
    ))
}

/// Gives each free acquisition a pass listed its order, unless it has one already, and never
/// rewrites one. A row above every placed row is new: it takes the pass's start, or for an
/// account none of whose rows is placed yet, the `FIRST_PASS` prefix. A row below a placed
/// one, which an earlier pass listed without placing (its page could not be read then), sorts
/// just above the nearest placed row under it. With `only`, a selected Update places just
/// those products, and only in an account a sync has placed. Returns the products whose
/// acquisitions changed.
pub(super) fn stage_free_orders(
    batch: &mut crate::booth::RecordBatch<'_>,
    places: &BTreeMap<String, FreePlaces>,
    only: Option<&BTreeSet<String>>,
) -> Result<BTreeSet<String>> {
    let mut products = BTreeSet::new();
    for (account, pass) in places {
        let mut rows: Vec<(u64, &String)> = pass
            .records
            .iter()
            .map(|(record, place)| (*place, record))
            .collect();
        rows.sort();
        let mut acquisitions = Vec::with_capacity(rows.len());
        for (_, record) in &rows {
            let id = format!("{account}:{record}");
            let acquisition = batch
                .get("acquisition", &id)?
                .filter(|acquisition| acquisition["source"] == "free");
            acquisitions.push((id, acquisition));
        }
        let kept: Vec<Option<String>> = acquisitions
            .iter()
            .map(|(_, acquisition)| {
                acquisition
                    .as_ref()
                    .and_then(|acquisition| acquisition["freeHistoryOrder"].as_str())
                    .map(str::to_owned)
            })
            .collect();
        // A free-history pass reads the whole list (only the Library stops early), so a pass
        // that lists no placed row is the account's first. An early stop here would have to
        // read this from the account's stored keys instead.
        let placed = kept.iter().any(Option::is_some);
        if only.is_some() && !placed {
            continue;
        }
        for (index, ((place, _), (id, acquisition))) in rows.iter().zip(acquisitions).enumerate() {
            let Some(mut acquisition) = acquisition else {
                continue;
            };
            let product = acquisition["productId"].as_str().unwrap_or("").to_owned();
            if kept[index].is_some() || only.is_some_and(|only| !only.contains(&product)) {
                continue;
            }
            let above = kept[..index].iter().any(Option::is_some);
            let below = kept[index + 1..].iter().flatten().next();
            let prefix = match (above, below) {
                (false, _) if placed => pass_prefix(&pass.started),
                (true, Some(below)) => Some(below.clone()),
                _ => Some(FIRST_PASS.to_owned()),
            };
            let Some(order) = prefix.and_then(|prefix| free_history_order(&prefix, *place)) else {
                continue;
            };
            acquisition["freeHistoryOrder"] = json!(order);
            batch.stage("acquisition", &id, acquisition)?;
            products.insert(product);
        }
    }
    products.remove("");
    Ok(products)
}

pub(super) fn augment_free_row(row: &mut parser::Row, evidence: &free_item::Evidence) {
    if row.product_id.as_deref() == Some(evidence.product_id.as_str()) {
        row.manifest_complete = false;
    }
    if row.product_id.as_deref() == Some(evidence.product_id.as_str())
        && !row.unavailable
        && row.error_code.is_none()
        && evidence.manifest_complete
        && !evidence.unavailable
        && evidence.error_code.is_none()
        && !evidence.files.is_empty()
    {
        row.files = evidence.files.clone();
        row.manifest_complete = true;
    }
}

pub(super) fn settled_free_absence(
    page: &parser::Page,
    history: &BTreeMap<String, Vec<parser::Row>>,
) -> bool {
    let Some(evidence) = &page.free_item else {
        return false;
    };
    page.source == "free"
        && page.is_detail
        && page.unresolved == 1
        && safety::free_item_id(&page.route) == Some(evidence.product_id.as_str())
        && evidence.unavailable
        && !evidence.manifest_complete
        && evidence.files.is_empty()
        && matches!(
            evidence.error_code.as_deref(),
            Some("FREE_OFFER_UNAVAILABLE" | "PUBLIC_METADATA_UNAVAILABLE")
        )
        && history.get(&evidence.product_id).is_some_and(|rows| {
            !rows.is_empty()
                && rows.iter().all(|row| {
                    row.product_id.as_deref() == Some(evidence.product_id.as_str())
                        && !row.source_record_id.is_empty()
                        && !row.unavailable
                        && row.error_code.is_none()
                })
        })
}
