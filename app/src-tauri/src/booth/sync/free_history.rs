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
