//! Resumable per-source scan checkpoints and their completion checks.
use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Checkpoint {
    pub(super) version: u32,
    pub(super) account_id: String,
    pub(super) source: String,
    pub(super) run_id: String,
    pub(super) mode: String,
    pub(super) pending: Vec<String>,
    pub(super) committed: BTreeMap<String, String>,
    pub(super) head: String,
    pub(super) boundary: Option<String>,
    pub(super) terminal: bool,
    pub(super) unresolved: usize,
    pub(super) product_ids: BTreeSet<String>,
    pub(super) enrichment_pending: BTreeSet<String>,
    pub(super) pages: usize,
    #[serde(default)]
    pub(super) details_completed: usize,
    pub(super) finished: bool,
    pub(super) journal_ids: Vec<String>,
    #[serde(default)]
    pub(super) ordering: parser::Ordering,
    #[serde(default)]
    pub(super) overlap_end: Option<usize>,
    pub(super) free_item_adapter_version: u32,
    pub(super) publication_version: u32,
    pub(super) published_products: BTreeSet<String>,
    /// Listed receipts whose rows come from the previous complete pass.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(super) reused: BTreeSet<String>,
    /// The Library's updated order showed what an overlap cannot explain: read every page.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) read_every_page: bool,
    /// When this run began: a pass finished long after its pages were read is no fresher.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) started_at: String,
}

impl Checkpoint {
    pub(super) fn fresh(account: &str, source: &str, mode: &str) -> Result<Self> {
        Ok(Self {
            version: 1,
            publication_version: 1,
            account_id: account.into(),
            source: source.into(),
            mode: mode.into(),
            run_id: uuid::Uuid::new_v4().to_string(),
            pending: vec![format!("{}?page=1", safety::source_path(source)?)],
            free_item_adapter_version: if source == "free" {
                FREE_ITEM_ADAPTER_VERSION
            } else {
                0
            },
            started_at: now(),
            ..Self::default()
        })
    }
    /// Only what this pass lists can be settled. A row seen only in updated order, which no
    /// plain page went on to show, would otherwise hold the source open for good.
    pub(super) fn forget_unlisted(&mut self, listed: impl Fn(&str) -> bool) {
        self.enrichment_pending.retain(|id| listed(id));
    }
    /// When a pass that read every page read them. A saved run from before this was recorded
    /// has only the time it finishes.
    pub(super) fn read_from(&self) -> String {
        if self.started_at.is_empty() {
            now()
        } else {
            self.started_at.clone()
        }
    }
    pub(super) fn complete(&self) -> bool {
        self.pending.is_empty()
            && self.terminal
            && self.unresolved == 0
            && self.enrichment_pending.is_empty()
    }
}

/// What a source scan left open. Only a listing it could not close keeps the job
/// incomplete; assets left as they were are counted instead.
#[derive(Default)]
pub(super) struct SourceScan {
    pub(super) settled: bool,
    /// Every gap is an asset the store refused or that could not be read.
    pub(super) gaps_local: bool,
    pub(super) products: BTreeSet<String>,
    pub(super) rows: usize,
}

impl SourceScan {
    pub(super) fn incomplete(&self) -> bool {
        !self.settled && !self.gaps_local
    }
}

/// The job completes with the number of assets and unreadable rows left as they were.
pub(super) fn record_incomplete(job: &mut Value, products: &BTreeSet<String>, rows: usize) {
    let count = products.len() + rows;
    if count > 0 {
        job["progress"]["incompleteProducts"] = json!(count);
    } else if let Some(progress) = job["progress"].as_object_mut() {
        progress.remove("incompleteProducts");
    }
}

pub(super) fn source_has_incomplete_product(
    checkpoint: &Checkpoint,
    scopes: &BTreeSet<(String, String, String)>,
) -> bool {
    scopes.iter().any(|(account, source, _)| {
        account == &checkpoint.account_id && source == &checkpoint.source
    })
}

pub(super) fn checkpoint_finished_after_collection(
    checkpoint: &Checkpoint,
    complete: bool,
    incomplete_scopes: &BTreeSet<(String, String, String)>,
) -> bool {
    complete
        || checkpoint.unresolved != 0
        || checkpoint.pending.is_empty()
            && checkpoint.terminal
            && source_has_incomplete_product(checkpoint, incomplete_scopes)
}

pub(super) fn resumable_checkpoint(
    saved: Option<Value>,
    mode: &str,
    selected: bool,
) -> Result<Option<Checkpoint>> {
    // A forced traversal or selected-product refresh never resumes this source
    // checkpoint. Do not let an unrelated malformed saved run block that intent.
    if mode == "forced" || selected {
        return Ok(None);
    }
    saved
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))
}

#[cfg(test)]
pub(super) fn source_traversal_verified(
    checkpoint: &Checkpoint,
    final_head: &parser::Page,
) -> bool {
    checkpoint.terminal
        && checkpoint.pending.is_empty()
        && checkpoint.unresolved == 0
        && !checkpoint.head.is_empty()
        && final_head.fingerprint == checkpoint.head
}
