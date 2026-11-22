//! The acquisition, remote file and entitlement records a BOOTH sync writes. Each is built
//! whole from what was observed, so a field kept from the record before is named here
//! instead of being copied back from a list. Gumroad, Jinxxy and local imports store their
//! own shapes under the same kinds.
use super::*;
use serde::de::DeserializeOwned;

/// One listing row of one account: an `acquisition` record.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Acquisition {
    pub(super) id: String,
    pub(super) source_record_id: String,
    /// The product whose files the row grants; a purchase line without files grants none.
    pub(super) product_id: Option<String>,
    /// The product the row names.
    pub(super) observed_product_id: Option<String>,
    pub(super) account_id: String,
    pub(super) source: String,
    pub(super) order_id: Option<String>,
    pub(super) acquired_date_text: Option<String>,
    /// When the App added a free item by its listing, to the millisecond, so items added
    /// together keep their order; the date shown stays `acquired_date_text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) acquired_at: Option<String>,
    pub(super) paid_amount_text: Option<String>,
    pub(super) variant_name: Option<String>,
    pub(super) unavailable: bool,
    pub(super) error_code: Option<String>,
    pub(super) observed_at: String,
    /// Numeric variation IDs of the row's files; `null` only when kept from a record that
    /// had none.
    pub(super) variant_ids: Option<Vec<String>>,
    /// The receipt's product-line total in yen, with `paidCurrency` and `paidPriceBasis`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) paid_price: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) paid_currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) paid_price_basis: Option<String>,
    /// What the one owned variation cost; `null` when kept from a record that had none.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) variant_payments: Option<Option<Vec<VariantPayment>>>,
    /// An undated free item's place in the free history (`free_history_order`), given
    /// once at a final commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) free_history_order: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VariantPayment {
    pub(super) variant_id: String,
    pub(super) name: Option<String>,
    pub(super) price: serde_json::Number,
    pub(super) currency: String,
}

impl Acquisition {
    /// The record of one listing row. A previous record of the same order and product keeps
    /// what this listing leaves blank (dates, amounts, the order and the products), a free
    /// item's place in the history, valid paid facts, and the payments of the same
    /// variations.
    pub(super) fn observe(
        id: String,
        account: &str,
        source: &str,
        row: &parser::Row,
        route: &str,
        previous: Option<&Value>,
    ) -> Self {
        let previous =
            previous.filter(|previous| same_acquisition_binding(previous, account, source, row));
        let kept = |key: &str| {
            previous
                .and_then(|previous| previous[key].as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        };
        // Sparse refreshes do not erase historical facts or their binding.
        let sparse = |observed: &Option<String>, key: &str| match observed {
            Some(value) if !value.trim().is_empty() => observed.clone(),
            _ => kept(key).or_else(|| observed.clone()),
        };
        let fileless = row.files.is_empty();
        let product_id = if source == "purchase" && fileless {
            kept("productId").filter(|id| safety::numeric(id))
        } else {
            row.product_id.clone()
        };
        let variant_ids: Vec<String> = row
            .files
            .iter()
            .filter_map(|f| f.variant_id.as_deref())
            .filter(|id| safety::numeric(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect();
        // A listing without files says nothing about variations.
        let variants_unseen = variant_ids.is_empty() && fileless;
        let line_total = native_product_line_total(row, source, route);
        let paid = previous.filter(|previous| {
            previous["paidCurrency"] == "JPY"
                && previous["paidPriceBasis"] == "productLineTotal"
                && previous["paidPrice"].is_number()
                && metadata::parse_jpy_price(&previous["paidPrice"]).is_some()
        });
        let paid_price = match (line_total, paid) {
            (Some(amount), _) => Some(amount.into()),
            (None, Some(paid)) => paid["paidPrice"].as_number().cloned(),
            (None, None) => None,
        };
        let paid_line = paid_price.is_some();
        let single_variant =
            variant_ids.len() == 1 && row.files.iter().all(|f| f.variant_id.is_some());
        let variant_payments = match (line_total.filter(|_| single_variant), previous) {
            (Some(amount), _) => Some(Some(vec![VariantPayment {
                variant_id: variant_ids[0].clone(),
                name: row.variant_name.clone(),
                price: amount.into(),
                currency: "JPY".into(),
            }])),
            (None, Some(previous)) if variants_unseen => {
                Some(kept_as(&previous["variantPayments"]))
            }
            (None, Some(previous)) if previous["variantIds"] == json!(variant_ids) => {
                previous.get("variantPayments").map(kept_as)
            }
            (None, _) => None,
        };
        Self {
            id,
            source_record_id: row.source_record_id.clone(),
            product_id,
            observed_product_id: sparse(&row.product_id, "observedProductId"),
            account_id: account.into(),
            source: source.into(),
            order_id: sparse(&row.order_id, "orderId"),
            acquired_date_text: sparse(&row.acquired_date_text, "acquiredDateText"),
            acquired_at: sparse(&row.acquired_at, "acquiredAt"),
            paid_amount_text: sparse(&row.paid_amount_text, "paidAmountText"),
            variant_name: row.variant_name.clone(),
            unavailable: row.unavailable,
            error_code: row.error_code.clone(),
            observed_at: now(),
            variant_ids: match previous {
                Some(previous) if variants_unseen => kept_as(&previous["variantIds"]),
                _ => Some(variant_ids),
            },
            paid_price,
            paid_currency: paid_line.then(|| "JPY".into()),
            paid_price_basis: paid_line.then(|| "productLineTotal".into()),
            variant_payments,
            free_history_order: kept("freeHistoryOrder"),
        }
    }
}

/// One downloadable an account is offered: a `remote_file` record. Storage and the
/// download ticket later set its revision, basis, verification flag and size in place.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteFile {
    pub(super) id: String,
    pub(super) product_id: String,
    pub(super) account_id: String,
    pub(super) source_file_id: String,
    pub(super) variant_id: Option<String>,
    pub(super) variant_name: Option<String>,
    pub(super) name: Option<String>,
    pub(super) size_bytes: Option<u64>,
    pub(super) revision: String,
    pub(super) revision_basis: String,
    pub(super) requires_content_verification: bool,
    pub(super) source_url: String,
    /// The page that last changed the file's facts; another listing of it is no change.
    pub(super) resolver_page: String,
    pub(super) source: String,
    pub(super) observed_at: String,
    /// Marks a descriptor this job already verified, so replaying it skips verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) source_observation_token: Option<String>,
    /// The one rendition requested from a multi-format downloadable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) download_format: Option<String>,
}

impl RemoteFile {
    /// The record of one listed downloadable. A previous record supplies the name and size
    /// this listing leaves out, and the page that last changed the file's facts.
    pub(super) fn observe(
        id: String,
        account: &str,
        product: &str,
        source: &str,
        row: &parser::Row,
        file: &parser::File,
        route: &str,
        existing: Option<&Value>,
        (revision, revision_basis, requires_content_verification): (String, String, bool),
        source_observation_token: Option<String>,
    ) -> Result<Self> {
        let mut remote = Self {
            id,
            product_id: product.into(),
            account_id: account.into(),
            source_file_id: file.id.clone(),
            variant_id: file.variant_id.clone(),
            variant_name: row.variant_name.clone(),
            name: file
                .name
                .clone()
                .or_else(|| existing.and_then(|e| e["name"].as_str()).map(str::to_owned)),
            size_bytes: file
                .size_bytes
                .or_else(|| existing.and_then(|e| e["sizeBytes"].as_u64())),
            revision,
            revision_basis,
            requires_content_verification,
            source_url: format!("https://booth.pm/downloadables/{}", file.id),
            resolver_page: route.into(),
            source: source.into(),
            observed_at: now(),
            source_observation_token,
            download_format: file.format.clone(),
        };
        // The Library and a receipt can list the same downloadable. Keep the
        // page that last changed its facts; another listing is not a change.
        let saved_listing = existing.and_then(|saved| {
            Some((
                saved,
                saved["resolverPage"].as_str()?,
                saved["source"].as_str()?,
            ))
        });
        if let Some((saved, page, listed_by)) = saved_listing {
            let mut moved = saved.clone();
            moved["resolverPage"] = json!(route);
            moved["source"] = json!(source);
            if crate::booth::batch::same_except_observed_at(&moved, &serde_json::to_value(&remote)?)
            {
                remote.resolver_page = page.into();
                remote.source = listed_by.into();
            }
        }
        Ok(remote)
    }
}

/// What one account holds of a product through one source: an `entitlement` record.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Entitlement {
    pub(super) id: String,
    pub(super) product_id: String,
    pub(super) account_id: String,
    pub(super) source: String,
    /// Every remote file listed so far, or the certified list.
    pub(super) file_ids: BTreeSet<String>,
    /// Numeric variation IDs of the certified list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) variant_ids: Option<BTreeSet<String>>,
    pub(super) manifest_complete: bool,
    pub(super) current_manifest: bool,
    /// Why the certified list counts as complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) manifest_basis: Option<String>,
    pub(super) observed_at: String,
    /// Whether the last listed row was unavailable; a certified list leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) unavailable: Option<bool>,
}

impl Entitlement {
    /// After a listed row: every file listed so far, not certified. A certified list's
    /// variations and basis stay until publication certifies again.
    pub(super) fn listed(
        id: String,
        account: &str,
        source: &str,
        product: &str,
        files: impl IntoIterator<Item = String>,
        unavailable: bool,
        existing: Option<&Value>,
    ) -> Self {
        let kept = |key: &str| existing.and_then(|e| e.get(key));
        let mut file_ids: BTreeSet<String> = kept("fileIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        file_ids.extend(files);
        Self {
            id,
            product_id: product.into(),
            account_id: account.into(),
            source: source.into(),
            file_ids,
            variant_ids: kept("variantIds").and_then(kept_as),
            manifest_complete: false,
            current_manifest: true,
            manifest_basis: kept("manifestBasis").and_then(kept_as),
            observed_at: now(),
            unavailable: Some(unavailable),
        }
    }

    /// At publication: the list this grant proves replaces what was listed before.
    pub(super) fn certified(
        id: String,
        account: &str,
        source: &str,
        product: &str,
        file_ids: BTreeSet<String>,
        variant_ids: BTreeSet<String>,
        complete: bool,
        basis: &str,
    ) -> Self {
        Self {
            id,
            product_id: product.into(),
            account_id: account.into(),
            source: source.into(),
            file_ids,
            variant_ids: Some(variant_ids),
            manifest_complete: complete,
            current_manifest: true,
            manifest_basis: Some(basis.into()),
            observed_at: now(),
            unavailable: None,
        }
    }
}

/// A stored value read as one of these types' fields; a value of another shape, which no
/// version has written, reads as missing.
pub(super) fn kept_as<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

/// Stages a record. Test builds check that it reads back as its type unchanged.
pub(super) fn stage<T: Serialize + DeserializeOwned>(
    batch: &mut crate::booth::RecordBatch<'_>,
    kind: &str,
    id: &str,
    record: &T,
) -> Result<()> {
    let value = serde_json::to_value(record)?;
    #[cfg(test)]
    check::<T>(kind, &value);
    batch.stage(kind, id, value)
}

/// Checks a stored record against its type, closed: a field the type lacks fails.
#[cfg(test)]
pub(super) fn check<T: Serialize + DeserializeOwned>(kind: &str, value: &Value) {
    crate::ipc::assert_view::<T>(kind, value, true, |_| vec![]);
}

/// Test builds check every BOOTH record of these kinds the catalogue writes, whoever writes
/// it: a field its type lacks would be dropped by the next sync, so it fails here instead.
#[cfg(test)]
pub(crate) fn assert_declared(kind: &str, value: &Value) {
    // BOOTH's records name no provider and belong to a numeric BOOTH product, or to none
    // for a purchase line without files.
    let booth = value.get("provider").is_none()
        && value.get("originKind").is_none()
        && value
            .get("productId")
            .is_some_and(|id| id.is_null() || id.as_str().is_some_and(safety::numeric));
    if !booth {
        return;
    }
    let fields = match kind {
        "acquisition" => crate::ipc::declared_fields::<Acquisition>(),
        "remote_file" => crate::ipc::declared_fields::<RemoteFile>(),
        "entitlement" => crate::ipc::declared_fields::<Entitlement>(),
        _ => return,
    };
    let undeclared: Vec<_> = value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key)
        .filter(|key| !fields.contains(&key.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "BOOTH {kind} has fields its type lacks: {undeclared:?}"
    );
}
