//! Closed structural counts only. This does not change or certify source ordering.
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

const ACQUIRED: &str = "入手が新しい順";
const UPDATED: &str = "ファイル更新が新しい順";
/// Independent failure counters overlap: one row may fail several gates.
/// Every value is a fixed number/boolean; source values stay inside the parser.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManifestDiagnostics {
    version: u32,
    rows: usize,
    gate_evaluated_rows: usize,
    rows_complete: usize,
    rows_missing_explicit_list: usize,
    rows_without_files: usize,
    rows_with_error: usize,
    rows_unavailable: usize,
    rows_invalid_controls: usize,
    rows_missing_filename: usize,
    raw_controls: usize,
    valid_controls: usize,
    duplicate_controls: usize,
    invalid_controls: usize,
    files: usize,
    files_with_variant: usize,
    files_named: usize,
    files_unnamed: usize,
    files_size_known: usize,
    files_positive_revision: usize,
    unresolved_rows: usize,
    detail_routes: usize,
    next_routes: usize,
    terminal_seen: bool,
}
impl ManifestDiagnostics {
    pub(super) fn record(
        &mut self,
        explicit_list: bool,
        files: &[super::parser::File],
        error_free: bool,
        unavailable: bool,
        raw: usize,
        valid: usize,
        complete: bool,
    ) {
        self.gate_evaluated_rows += 1;
        self.rows_complete += usize::from(complete);
        self.rows_missing_explicit_list += usize::from(!explicit_list);
        self.rows_without_files += usize::from(files.is_empty());
        self.rows_with_error += usize::from(!error_free);
        self.rows_unavailable += usize::from(unavailable);
        self.rows_invalid_controls += usize::from(raw != valid);
        self.rows_missing_filename += usize::from(files.iter().any(|f| f.name.is_none()));
        self.raw_controls += raw;
        self.valid_controls += valid;
        self.duplicate_controls += valid.saturating_sub(files.len());
        self.invalid_controls += raw.saturating_sub(valid);
        self.files += files.len();
        self.files_with_variant += files.iter().filter(|f| f.variant_id.is_some()).count();
        self.files_named += files.iter().filter(|f| f.name.is_some()).count();
        self.files_unnamed += files.iter().filter(|f| f.name.is_none()).count();
        self.files_size_known += files.iter().filter(|f| f.size_bytes.is_some()).count();
        self.files_positive_revision += files
            .iter()
            .filter(|f| !f.revision.is_empty() && f.revision != "unknown")
            .count();
    }
    pub(super) fn finish(&mut self, page: &super::parser::Page) {
        self.version = 1;
        self.rows = page.rows.len();
        self.unresolved_rows = page.unresolved;
        self.detail_routes = page.details.len();
        self.next_routes = page.next.len();
        self.terminal_seen = page.terminal;
    }
}
fn sel(value: &str) -> Selector {
    Selector::parse(value).expect("fixed selector")
}
fn text(node: ElementRef<'_>) -> String {
    super::safety::short(&node.text().collect::<String>())
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LabelCounts {
    acquired_exact: u32,
    acquired_containing_only: u32,
    file_updated_exact: u32,
    file_updated_containing_only: u32,
}
impl LabelCounts {
    fn add(&mut self, label: &str) {
        if label == ACQUIRED {
            self.acquired_exact += 1;
        } else if label.contains(ACQUIRED) {
            self.acquired_containing_only += 1;
        }
        if label == UPDATED {
            self.file_updated_exact += 1;
        } else if label.contains(UPDATED) {
            self.file_updated_containing_only += 1;
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Language {
    Ja,
    En,
    Ko,
    ZhHans,
    ZhHant,
    Unknown,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OrderingDiagnostics {
    version: u32,
    language: Language,
    search_value_nonempty: bool,
    selects: LabelCounts,
    options: LabelCounts,
    buttons: LabelCounts,
    links: LabelCounts,
    radios: LabelCounts,
    selected_attribute: LabelCounts,
    aria_pressed: LabelCounts,
    aria_selected: LabelCounts,
    aria_current: LabelCounts,
    aria_expanded_present: LabelCounts,
    radio_checked: LabelCounts,
    implicit_single_select_first: LabelCounts,
}
pub(super) fn inspect(body: &str) -> OrderingDiagnostics {
    let doc = Html::parse_document(body);
    let language = match doc
        .select(&sel("html"))
        .next()
        .and_then(|e| e.value().attr("lang"))
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "ja" | "ja-jp" => Language::Ja,
        "en" | "en-us" | "en-gb" => Language::En,
        "ko" | "ko-kr" => Language::Ko,
        "zh-hans" | "zh-cn" => Language::ZhHans,
        "zh-hant" | "zh-tw" | "zh-hk" => Language::ZhHant,
        _ => Language::Unknown,
    };
    let mut result = OrderingDiagnostics {
        version: 1,
        language,
        search_value_nonempty: doc
            .select(&sel(
                "input[type='search'][value],input[name='q'][value],input[name='keyword'][value]",
            ))
            .any(|e| {
                e.value()
                    .attr("value")
                    .is_some_and(|v| !v.trim().is_empty())
            }),
        selects: Default::default(),
        options: Default::default(),
        buttons: Default::default(),
        links: Default::default(),
        radios: Default::default(),
        selected_attribute: Default::default(),
        aria_pressed: Default::default(),
        aria_selected: Default::default(),
        aria_current: Default::default(),
        aria_expanded_present: Default::default(),
        radio_checked: Default::default(),
        implicit_single_select_first: Default::default(),
    };
    for node in doc.select(&sel("select,option,button,a,input[type='radio'],[aria-pressed],[aria-selected],[aria-current],[aria-expanded]")) {
        let is_radio = node.value().name() == "input"
            && node.value().attr("type").is_some_and(|v| v.eq_ignore_ascii_case("radio"));
        let label = if is_radio {
            // Only a native radio's associated label is examined; no attribute
            // values or text are ever returned, even for unknown controls.
            node.ancestors().filter_map(ElementRef::wrap).find(|p| p.value().name() == "label").map(text)
                .or_else(|| node.value().attr("id").and_then(|id| doc.select(&sel("label[for]")).find(|l| l.value().attr("for") == Some(id)).map(text)))
                .unwrap_or_default()
        } else { text(node) };
        match node.value().name() {
            "select" => result.selects.add(&label),
            "option" => result.options.add(&label),
            "button" => result.buttons.add(&label),
            "a" => result.links.add(&label),
            "input" if is_radio => { result.radios.add(&label); if node.value().attr("checked").is_some() { result.radio_checked.add(&label); } },
            _ => (),
        }
        if node.value().name() == "option" && node.value().attr("selected").is_some() { result.selected_attribute.add(&label); }
        if node.value().attr("aria-pressed") == Some("true") { result.aria_pressed.add(&label); }
        if node.value().attr("aria-selected") == Some("true") { result.aria_selected.add(&label); }
        if node.value().attr("aria-current").is_some_and(|v| !v.is_empty() && v != "false") { result.aria_current.add(&label); }
        if node.value().attr("aria-expanded").is_some() { result.aria_expanded_present.add(&label); }
        if node.value().name() == "select"
            && node.value().attr("multiple").is_none()
            && node.value().attr("disabled").is_none()
            && node.value().attr("size").is_none_or(|v| v == "1")
            && node.select(&sel("option[selected]")).next().is_none()
        {
            if let Some(first) = node.select(&sel("option")).next() {
                let disabled_group = first.ancestors().filter_map(ElementRef::wrap).take_while(|p| p.id() != node.id()).any(|p| p.value().name() == "optgroup" && p.value().attr("disabled").is_some());
                if first.value().attr("disabled").is_none() && !disabled_group { result.implicit_single_select_first.add(&text(first)); }
            }
        }
    }
    result
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
