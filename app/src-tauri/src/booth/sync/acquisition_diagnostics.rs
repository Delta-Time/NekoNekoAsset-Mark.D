//! Targeted acquisition and free-item diagnostics.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum FreeItemDiagnostic {
    Entry,
    Identity,
    Role,
    Controls,
}

impl FreeItemDiagnostic {
    pub(super) fn error_code(self) -> &'static str {
        match self {
            Self::Entry => "FREE_ITEM_ENTRY_UNPROVEN",
            Self::Identity => "FREE_ITEM_IDENTITY_UNPROVEN",
            Self::Role => "FREE_ITEM_ROLE_UNPROVEN",
            Self::Controls => "FREE_FILE_CONTROLS_UNAVAILABLE",
        }
    }
}

// Reserve existing job keys without holding the mutex across native/network awaits.
pub(super) struct DiagnosticReservation<'a> {
    pub(super) running: &'a Mutex<BTreeSet<String>>,
    pub(super) account_key: String,
}

impl<'a> DiagnosticReservation<'a> {
    pub(super) fn acquire(running: &'a Mutex<BTreeSet<String>>, account: &str) -> Result<Self> {
        let account_key = format!("account:{account}");
        let mut active = running
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        if !active.is_empty() {
            bail!("ACCOUNT_BUSY");
        }
        active.insert("booth-global".into());
        active.insert(account_key.clone());
        Ok(Self {
            running,
            account_key,
        })
    }
}

impl Drop for DiagnosticReservation<'_> {
    fn drop(&mut self) {
        let mut active = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        active.remove("booth-global");
        active.remove(&self.account_key);
    }
}

pub(super) fn select_acquisition_diagnostic_detail(
    checkpoint: &Checkpoint,
    journals: &[Value],
    account: &str,
) -> Result<(String, Vec<parser::Row>)> {
    acquisition_diagnostic_details(checkpoint, journals, account)?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("DIAGNOSTIC_ENTRY_NOT_FOUND"))
}

pub(super) fn select_targeted_receipt_proof(
    checkpoint: &Checkpoint,
    journals: &[Value],
    account: &str,
    receipt_id: &str,
) -> Result<Option<Vec<parser::Row>>> {
    if !safety::numeric(receipt_id) {
        bail!("INVALID_DIAGNOSTIC_ARGUMENTS");
    }
    let target = format!("/orders/{receipt_id}");
    // The existing validator processes ALL journals, never just the target.
    Ok(
        acquisition_diagnostic_details(checkpoint, journals, account)?
            .into_iter()
            .find(|(route, _)| *route == target)
            .map(|(_, proof)| proof),
    )
}

pub(super) fn acquisition_diagnostic_details(
    checkpoint: &Checkpoint,
    journals: &[Value],
    account: &str,
) -> Result<Vec<(String, Vec<parser::Row>)>> {
    if checkpoint.version != 1
        || checkpoint.account_id != account
        || checkpoint.source != "purchase"
        || checkpoint.run_id.is_empty()
        || journals.len() != checkpoint.journal_ids.len()
    {
        bail!("CHECKPOINT_INVALID");
    }
    let mut witnessed = BTreeSet::new();
    let mut pages = Vec::new();
    for (id, value) in checkpoint.journal_ids.iter().zip(journals) {
        if value["id"] != id.as_str() {
            bail!("CHECKPOINT_INVALID");
        }
        let page = checked_journal_page(value, account, "purchase", &checkpoint.run_id)?;
        if checkpoint.committed.get(&page.route) != Some(&page.fingerprint) {
            bail!("CHECKPOINT_INVALID");
        }
        let url = safety::route(&page.route, "purchase")
            .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        if page.is_detail != (url.path() != "/orders") {
            bail!("CHECKPOINT_INVALID");
        }
        if !page.is_detail {
            for route in &page.details {
                let url = safety::route(route, "purchase")
                    .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
                if url.path() == "/orders" {
                    bail!("CHECKPOINT_INVALID");
                }
                witnessed.insert(route.clone());
            }
        }
        pages.push(page);
    }
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for page in pages {
        if !page.is_detail || !witnessed.contains(&page.route) {
            continue;
        }
        let receipt_id = page
            .route
            .strip_prefix("/orders/")
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let proof: Vec<_> = page
            .rows
            .into_iter()
            .filter(|row| {
                row.order_id.as_deref() == Some(receipt_id)
                    && parser::diagnostic_digital_file_keys(row).is_some()
            })
            .collect();
        if !proof.is_empty() && seen.insert(page.route.clone()) {
            result.push((page.route, proof));
        }
    }
    Ok(result)
}

#[cfg(test)]
pub(super) fn select_entry_diagnostic_route(
    checkpoint: &Checkpoint,
    journals: &[Value],
    account: &str,
) -> Result<String> {
    select_free_item_diagnostic_route(checkpoint, journals, account, FreeItemDiagnostic::Entry)
}

pub(super) fn select_free_item_diagnostic_route(
    checkpoint: &Checkpoint,
    journals: &[Value],
    account: &str,
    issue: FreeItemDiagnostic,
) -> Result<String> {
    if checkpoint.version != 1
        || checkpoint.free_item_adapter_version != FREE_ITEM_ADAPTER_VERSION
        || checkpoint.account_id != account
        || checkpoint.source != "free"
        || checkpoint.run_id.is_empty()
        || journals.len() != checkpoint.journal_ids.len()
    {
        bail!("CHECKPOINT_INVALID");
    }
    let mut history = BTreeMap::new();
    let mut pages = vec![];
    for (id, value) in checkpoint.journal_ids.iter().zip(journals) {
        if value["id"] != id.as_str() {
            bail!("CHECKPOINT_INVALID");
        }
        let page = checked_journal_page(value, account, "free", &checkpoint.run_id)?;
        if checkpoint.committed.get(&page.route) != Some(&page.fingerprint) {
            bail!("CHECKPOINT_INVALID");
        }
        remember_free_history(&page, &mut history);
        pages.push(page);
    }
    pages
        .into_iter()
        .find_map(|page| {
            page.free_item
                .filter(|e| {
                    e.error_code.as_deref() == Some(issue.error_code())
                        && history.contains_key(&e.product_id)
                })
                .map(|_| page.route)
        })
        .ok_or_else(|| anyhow::anyhow!("DIAGNOSTIC_ENTRY_NOT_FOUND"))
}
