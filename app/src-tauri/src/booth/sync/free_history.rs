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

/// A sort key for an undated free download, kept from the pass that first listed it: a
/// later pass sorts after an earlier one, and within one pass BOOTH's newest-first
/// position decides. Larger is newer.
pub(super) fn free_history_order(started_at: &str, position: u64) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    Some(format!(
        "{}{:012}",
        started.with_timezone(&chrono::Utc).format("%Y%m%d%H%M%S"),
        999_999_999_999u64.checked_sub(position)?
    ))
}

/// Gives each free acquisition a pass listed its order, unless it has one already. Returns
/// the products whose acquisitions changed.
pub(super) fn stage_free_orders(
    batch: &mut crate::booth::RecordBatch<'_>,
    orders: &BTreeMap<String, String>,
) -> Result<BTreeSet<String>> {
    let mut products = BTreeSet::new();
    for (id, order) in orders {
        let Some(mut acquisition) = batch.get("acquisition", id)? else {
            continue;
        };
        if acquisition["source"] != "free" || !acquisition["freeHistoryOrder"].is_null() {
            continue;
        }
        acquisition["freeHistoryOrder"] = json!(order);
        if let Some(product) = acquisition["productId"].as_str() {
            products.insert(product.to_owned());
        }
        batch.stage("acquisition", id, acquisition)?;
    }
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
