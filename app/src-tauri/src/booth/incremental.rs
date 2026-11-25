//! Provider-ordered acquisition overlap. All stored facts are digital projections.
use super::{
    parser::{Ordering, Page, Row},
    safety,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Baseline {
    pub version: u32,
    pub account_id: String,
    pub source: String,
    pub complete: bool,
    pub ordering: Ordering,
    pub rows: Vec<Row>,
    pub full_traversal_at: String,
    /// Receipt routes this complete pass read, including receipts without digital rows.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub details: BTreeSet<String>,
    /// The rows that led the updated order last time, by `content`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated_head: Vec<String>,
}
impl Baseline {
    /// An incremental purchase scan still lists every order, but a receipt this
    /// complete pass already read supplies its rows instead of a new request.
    /// Orders are immutable; Full rereads every receipt.
    pub fn known_receipts(&self, account: &str, source: &str) -> BTreeMap<String, Vec<Row>> {
        let mut known: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        if source != "purchase"
            || self.version != 1
            || self.account_id != account
            || self.source != source
            || !self.complete
            || !unique(&self.rows)
            || !self.rows.iter().all(stable)
        {
            return known;
        }
        for row in &self.rows {
            let Some(order) = row.order_id.as_deref().filter(|id| safety::numeric(id)) else {
                // A row that cannot name its receipt cannot be reused safely.
                return BTreeMap::new();
            };
            known
                .entry(format!("/orders/{order}"))
                .or_default()
                .push(row.clone());
        }
        for route in &self.details {
            if route.strip_prefix("/orders/").is_some_and(safety::numeric) {
                known.entry(route.clone()).or_default();
            }
        }
        known
    }
    pub fn usable(&self, account: &str, source: &str) -> bool {
        self.version == 1
            && self.account_id == account
            && self.source == source
            && self.complete
            && self.ordering == Ordering::AcquiredNewestFirst
            && !self.rows.is_empty()
            && self.rows.iter().all(|row| stable(row) || withheld(row))
    }
    /// What neither order brings to the top, like a delisted row or a removed file, waits for
    /// the next pass that reads every page. This keeps that wait short.
    pub fn recently_read_in_full(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.full_traversal_at).is_ok_and(|at| {
            let age = now.signed_duration_since(at);
            // A date ahead of this clock proves nothing about how long ago that was.
            age >= chrono::Duration::zero() && age < chrono::Duration::days(FULL_PASS_DAYS)
        })
    }
    /// A complete pass that listed or read anything, so a list reading empty now contradicts it.
    pub fn found_something(&self) -> bool {
        !self.rows.is_empty() || !self.details.is_empty()
    }
}
const FULL_PASS_DAYS: i64 = 7;
fn stable(row: &Row) -> bool {
    !row.source_record_id.is_empty() && row.product_id.is_some() && row.error_code.is_none()
}
/// A delisted row names no product, and its record ID moves with its place in the list.
fn withheld(row: &Row) -> bool {
    row.product_id.is_none() && row.error_code.as_deref() == Some("BOOTH_WITHHELD_PRODUCT_ID")
}
pub(super) fn unique(rows: &[Row]) -> bool {
    let mut ids = BTreeSet::new();
    rows.iter().all(|r| ids.insert(&r.source_record_id))
}
/// What a row says about its product. A record ID stays out: the same row takes an
/// `:occurrence:` suffix or not depending on which page it falls on.
fn content(row: &Row) -> String {
    safety::hash(
        serde_json::to_vec(&serde_json::json!({
            "productId":row.product_id,"files":row.files,
            "manifestComplete":row.manifest_complete,"unavailable":row.unavailable,
            "errorCode":row.error_code
        }))
        .expect("digital row serialization"),
    )
}
/// One stable row of the previous pass. `position` counts only such rows, so a delisted row
/// between two others does not read as a gap; `index` is its place in the baseline.
struct KnownRow {
    position: usize,
    index: usize,
    content: String,
}
pub(super) struct Tracker {
    ordering: Ordering,
    /// By product: a product bought more than once can list several rows.
    known: BTreeMap<String, Vec<KnownRow>>,
    read: BTreeMap<String, BTreeSet<String>>,
    last_known: Option<usize>,
    matched: Option<usize>,
    matched_index: Option<usize>,
    matched_pages: usize,
    disabled: bool,
}
impl Tracker {
    pub fn new(base: &Baseline) -> Self {
        let mut known: BTreeMap<String, Vec<KnownRow>> = BTreeMap::new();
        let rows = base
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !withheld(row));
        for (position, (index, row)) in rows.enumerate() {
            known
                .entry(row.product_id.clone().unwrap_or_default())
                .or_default()
                .push(KnownRow {
                    position,
                    index,
                    content: content(row),
                });
        }
        Self {
            ordering: base.ordering,
            known,
            read: BTreeMap::new(),
            last_known: None,
            matched: None,
            matched_index: None,
            matched_pages: 0,
            disabled: !base.complete || base.ordering == Ordering::Unknown,
        }
    }
    /// A changed known row is refreshed, then requires a new two-page overlap.
    /// Unknown rows after the old boundary or reversed old identities invalidate order.
    pub fn observe(&mut self, page: &Page) -> Option<usize> {
        if self.disabled {
            return None;
        }
        if page.is_detail
            || page.ordering != self.ordering
            || page.unresolved != 0
            || !page.details.is_empty()
        {
            self.disabled = true;
            return None;
        }
        let mut matched_this_page = false;
        for row in page.rows.iter().filter(|row| !withheld(row)) {
            let Some(product) = row.product_id.as_ref().filter(|_| stable(row)) else {
                self.disabled = true;
                return None;
            };
            let current = content(row);
            self.read
                .entry(product.clone())
                .or_default()
                .insert(current.clone());
            let Some(entries) = self.known.get(product) else {
                if self.last_known.is_some() {
                    self.disabled = true;
                    return None;
                }
                self.matched = None;
                self.matched_pages = 0;
                continue;
            };
            let last = self.last_known;
            let mut later = entries
                .iter()
                .filter(|entry| last.is_none_or(|last| entry.position > last));
            let same = later.clone().find(|entry| entry.content == current);
            let Some(entry) = same.or_else(|| later.next()) else {
                self.disabled = true;
                return None;
            };
            let (position, index, changed) =
                (entry.position, entry.index, entry.content != current);
            self.last_known = Some(position);
            if changed {
                self.matched = None;
                self.matched_pages = 0;
                matched_this_page = false;
                continue;
            }
            if self.matched.is_none_or(|last| position != last + 1) {
                self.matched_pages = 0;
                matched_this_page = false;
            }
            self.matched = Some(position);
            self.matched_index = Some(index);
            if !matched_this_page {
                self.matched_pages += 1;
                matched_this_page = true;
            }
        }
        if page.rows.is_empty() {
            return None;
        }
        (self.matched_pages >= 2 && matched_this_page)
            .then_some(self.matched_index?)
            .filter(|_| !self.disabled)
    }
    /// Stopping at the overlap leaves rows further down unread, and a seller can change the
    /// files on any of them. BOOTH's "recently updated first" order brings those to the top.
    pub fn updated(&self, head: &[String]) -> UpdatedTracker {
        UpdatedTracker {
            known: self
                .known
                .iter()
                .map(|(product, rows)| {
                    (
                        product.clone(),
                        rows.iter().map(|row| row.content.clone()).collect(),
                    )
                })
                .collect(),
            read: self.read.clone(),
            head: head.to_vec(),
            last_head: None,
            head_run: 0,
            quiet_pages: 0,
            settled: false,
        }
    }
}
/// A page's worth of the last read's rows, in order, shows the read has passed everything
/// BOOTH moved since. The next pass keeps enough rows to find that after some have moved.
const HEAD_RUN: usize = 10;
const HEAD_KEPT: usize = 40;
#[derive(Debug, PartialEq)]
pub(super) enum UpdatedRead {
    Reading,
    Settled,
    /// Something this order should not show: the App reads every page instead.
    Broken,
}
pub(super) struct UpdatedTracker {
    known: BTreeMap<String, Vec<String>>,
    read: BTreeMap<String, BTreeSet<String>>,
    head: Vec<String>,
    last_head: Option<usize>,
    head_run: usize,
    quiet_pages: usize,
    settled: bool,
}
impl UpdatedTracker {
    /// Whether this row says what neither the last pass nor this pass's plain pages did.
    pub fn changes(&self, row: &Row) -> bool {
        let Some(product) = row.product_id.as_ref() else {
            return false;
        };
        let current = content(row);
        !self
            .known
            .get(product)
            .is_some_and(|rows| rows.contains(&current))
            && !self
                .read
                .get(product)
                .is_some_and(|rows| rows.contains(&current))
    }
    /// A row BOOTH moved sits above every row it did not, whether or not anything this account
    /// sees changed, so an unchanged row alone proves nothing. Reaching the rows that led this
    /// order last time does. Before any such read, two pages of the last pass's rows have to do.
    pub fn observe(&mut self, page: &Page) -> UpdatedRead {
        if page.is_detail
            || page.ordering != Ordering::FileUpdatedNewestFirst
            || page.unresolved != 0
            || !page.details.is_empty()
            || page.rows.is_empty()
        {
            return UpdatedRead::Broken;
        }
        let (mut quiet, mut rows) = (true, 0);
        for row in page.rows.iter().filter(|row| !withheld(row)) {
            let Some(product) = row.product_id.as_ref().filter(|_| stable(row)) else {
                return UpdatedRead::Broken;
            };
            rows += 1;
            let current = content(row);
            let known = self.known.get(product);
            let unchanged = known.is_some_and(|rows| rows.contains(&current));
            if !unchanged
                && !self
                    .read
                    .get(product)
                    .is_some_and(|rows| rows.contains(&current))
            {
                // A product no list has shown was acquired out of order or just now, and among
                // several rows of one product, which one changed is unknown.
                if known.is_none_or(|rows| rows.len() != 1) {
                    return UpdatedRead::Broken;
                }
            }
            quiet &= unchanged;
            self.follow_head(&current, unchanged);
        }
        // A page of delisted rows says nothing either way.
        if rows > 0 {
            self.quiet_pages = if quiet { self.quiet_pages + 1 } else { 0 };
        }
        if self.settled || (self.head.is_empty() && self.quiet_pages >= 2) {
            UpdatedRead::Settled
        } else if page.terminal || !page.next.is_empty() {
            UpdatedRead::Reading
        } else {
            UpdatedRead::Broken
        }
    }
    fn follow_head(&mut self, current: &str, unchanged: bool) {
        let place = |from: usize| {
            self.head[from..]
                .iter()
                .position(|row| row == current)
                .map(|place| place + from)
                .filter(|_| unchanged)
        };
        if let Some(next) = self.last_head.and_then(|last| place(last + 1)) {
            self.last_head = Some(next);
            self.head_run += 1;
        } else {
            self.last_head = place(0);
            self.head_run = usize::from(self.last_head.is_some());
        }
        self.settled |= !self.head.is_empty() && self.head_run >= HEAD_RUN.min(self.head.len());
    }
}
/// What the next pass looks for: this read's rows, then the last read's that are below them.
pub(super) fn next_head(updated: &[Row], previous: &[String]) -> Vec<String> {
    let mut head: Vec<_> = updated
        .iter()
        .filter(|row| !withheld(row))
        .map(content)
        .collect();
    for row in previous {
        if !head.contains(row) {
            head.push(row.clone());
        }
    }
    head.truncate(HEAD_KEPT);
    head
}
pub(super) struct Merged {
    pub rows: Vec<Row>,
    /// Rows read in updated order that say something new.
    pub changed: Vec<Row>,
}
/// The rows read in acquired order, the unread tail of the previous pass after them, and each
/// changed row read in updated order in its product's place. `UpdatedTracker` lets a change
/// through only for a product with one row.
pub(super) fn merged_rows(
    base: &Baseline,
    prefix: &[Row],
    overlap_end: usize,
    updated: &[Row],
) -> Option<Merged> {
    if overlap_end >= base.rows.len() {
        return None;
    }
    let mut rows = prefix.to_vec();
    rows.extend(
        base.rows[overlap_end + 1..]
            .iter()
            .filter(|row| !withheld(row))
            .cloned(),
    );
    let mut places = BTreeMap::new();
    for (place, row) in rows.iter().enumerate() {
        if let Some(product) = &row.product_id {
            places.entry(product.clone()).or_insert(place);
        }
    }
    let known: BTreeSet<_> = base.rows.iter().chain(prefix).map(content).collect();
    let mut changed = vec![];
    for row in updated
        .iter()
        .filter(|row| !withheld(row) && !known.contains(&content(row)))
    {
        let Some(product) = &row.product_id else {
            continue;
        };
        changed.push(row.clone());
        if let Some(place) = places.get(product) {
            rows[*place] = row.clone();
        } else {
            places.insert(product.clone(), rows.len());
            rows.push(row.clone());
        }
    }
    Some(Merged { rows, changed })
}

#[cfg(test)]
#[path = "incremental_tests.rs"]
mod tests;
