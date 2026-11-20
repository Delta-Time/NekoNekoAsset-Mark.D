//! Original, narrow free-item HTML adapter. Only observed free file controls grant files.
//! Provider behavior reference: official lilToon BOOTH item page, read 2026-10-09.
use super::{
    parser::{File, Ordering, Page},
    safety,
};
use anyhow::{bail, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const NAMES: &str = ".text-14.text-text-default.font-normal.text-left.mb-8";
const CONTROLS: &str = "a[href*='/downloadables/'],[data-href*='/downloadables/']";
const PAUSED_CART: &str = "button.add-cart.disabled[type='button']";
const RESTOCK_NOTICE: &str = ".restock-request-links a.js-add-restock-request,.restock-request-links a.js-remove-restock-request";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Evidence {
    pub product_id: String,
    pub files: Vec<File>,
    pub manifest_complete: bool,
    pub unavailable: bool,
    pub error_code: Option<String>,
}
impl Evidence {
    pub fn unknown(id: &str, code: &str, unavailable: bool) -> Self {
        Self {
            product_id: id.into(),
            files: vec![],
            manifest_complete: false,
            unavailable,
            error_code: Some(code.into()),
        }
    }
    pub fn page(self) -> Result<Page> {
        let fingerprint = safety::hash(
            serde_json::to_vec(&self).map_err(|_| anyhow::anyhow!("FREE_ITEM_INVALID"))?,
        );
        Ok(Page {
            source: "free".into(),
            route: safety::free_item_route(&self.product_id)?,
            rows: vec![],
            next: vec![],
            details: vec![],
            terminal: false,
            unresolved: usize::from(!self.manifest_complete),
            fingerprint,
            is_detail: true,
            ordering: Ordering::Unknown,
            free_item: Some(self),
        })
    }
}
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("fixed selector")
}
fn text(node: ElementRef<'_>) -> String {
    node.text().collect::<String>().trim().to_owned()
}
fn positive_row_price(row: ElementRef<'_>) -> bool {
    let prices: Vec<_> = row.select(&sel(".variation-price")).collect();
    prices.len() == 1 && {
        let price = text(prices[0]);
        (price.starts_with('¥') || price.ends_with('円') || price.ends_with("JPY"))
            && super::metadata::parse_jpy_price(&serde_json::json!(price))
                .is_some_and(|amount| amount > 0.0)
    }
}
fn non_free_role(row: ElementRef<'_>, id: &str) -> bool {
    row.select(&sel(
        "form button.add-cart[data-product-id],form button.add-gift[data-product-id]",
    ))
    .any(|button| button.value().attr("data-product-id") == Some(id))
        || (positive_row_price(row)
            && (row.select(&sel(PAUSED_CART)).next().is_some()
                || row.select(&sel(RESTOCK_NOTICE)).next().is_some()))
}
fn clean_name(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 4096 && !value.chars().any(char::is_control)
}
fn control(node: ElementRef<'_>) -> Option<(String, String, String)> {
    if node.value().name() != "a" || node.value().attr("data-href").is_some() {
        return None;
    }
    let raw = node.value().attr("href")?;
    if !raw.starts_with("https://booth.pm/downloadables/") {
        return None;
    }
    let (id, variant, canonical) = safety::resolver(raw)?;
    if canonical != raw {
        return None;
    }
    Some((id, variant?, canonical))
}
/// An alternate app menu for one bound file. Its optional `type` names that
/// file's own extension (`zip`, `vroid`, `vroidcustomitem`), never another file.
fn alternate(node: ElementRef<'_>, file: &str, variant: &str, name: &str) -> bool {
    if node.value().attr("data-test") != Some("other-downloads-button")
        || !node
            .value()
            .classes()
            .any(|c| c == "js-download-free-button")
    {
        return false;
    }
    let Some(items) = node
        .value()
        .attr("data-dropdown-items")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return false;
    };
    let Some(items) = items.as_array().filter(|a| !a.is_empty() && a.len() <= 10) else {
        return false;
    };
    items.iter().all(|item| {
        let Some(raw) = item["deeplinkDownloadableUrl"].as_str() else {
            return false;
        };
        let Ok(url) = url::Url::parse(raw) else {
            return false;
        };
        if !raw.starts_with("https://booth.pm/downloadables/")
            || raw.split('?').next()
                != Some(format!("https://booth.pm/downloadables/{file}/deeplink").as_str())
            || !safety::secure(&url)
            || url.host_str() != Some("booth.pm")
            || url.path() != format!("/downloadables/{file}/deeplink")
            || url.fragment().is_some()
        {
            return false;
        }
        let pairs: Vec<_> = url.query_pairs().collect();
        let mut keys = BTreeSet::new();
        pairs.iter().all(|(k, v)| {
            keys.insert(k.to_string())
                && match k.as_ref() {
                    "variation_id" => v.as_ref() == variant,
                    "client" => matches!(v.as_ref(), "booth-library-manager" | "vroid"),
                    "type" => name.rsplit_once('.').is_some_and(|(_, ext)| {
                        !ext.is_empty() && ext.eq_ignore_ascii_case(v.as_ref())
                    }),
                    _ => false,
                }
        }) && keys.contains("variation_id")
            && keys.contains("client")
    })
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EntryDiagnostics {
    version: u32,
    identity_bound: bool,
    entry_still_unproven: bool,
    current_parser_complete: bool,
    variation_rows: usize,
    recognized_non_free_rows: usize,
    unrecognized_role_rows: usize,
    static_disabled_rows: usize,
    unavailable_marker_rows: usize,
    positive_price_rows: usize,
    bound_paid_variation_rows: usize,
    disabled_cart_rows: usize,
    restock_notice_rows: usize,
    raw_controls: usize,
    valid_controls: usize,
    free_controls: usize,
    controls_missing_cart_wrap: usize,
    wraps_with_multiple_valid_controls: usize,
    wraps_without_filename: usize,
    wraps_with_multiple_filenames: usize,
    title_agreements: usize,
    inner_single_control_single_filename_scopes_within4_ancestors: usize,
    inner_single_control_single_filename_scopes_within8_ancestors: usize,
    distinct_matched_filename_nodes: usize,
    orphan_controls: usize,
    orphan_names: usize,
    known_alternate_methods: usize,
    unknown_dropdowns: usize,
    expansion_hints: usize,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IdentityDiagnostics {
    version: u32,
    canonical_count: usize,
    og_count: usize,
    root_count: usize,
    list_count: usize,
    lists_within_matching_root: usize,
    root_id_match: bool,
    canonical_exact: bool,
    og_exact: bool,
    canonical_old_bare_same_id: bool,
    og_old_bare_same_id: bool,
    canonical_locale_ja_same_id: bool,
    og_locale_ja_same_id: bool,
    canonical_locale_en_same_id: bool,
    og_locale_en_same_id: bool,
    canonical_shop_host_same_id: bool,
    og_shop_host_same_id: bool,
    canonical_other_shape: bool,
    og_other_shape: bool,
}
fn shop_item_identity(raw: &str, id: &str) -> bool {
    if raw.contains('\\') || raw.chars().any(char::is_control) {
        return false;
    }
    let Some(rest) = raw.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains([':', '@']) {
        return false;
    }
    let Ok(url) = url::Url::parse(raw) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if !safety::secure(&url)
        || !safety::booth_host(host)
        || matches!(
            host,
            "booth.pm" | "www.booth.pm" | "accounts.booth.pm" | "manage.booth.pm"
        )
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let path = rest.strip_prefix(authority).unwrap_or("");
    [
        format!("/items/{id}"),
        format!("/ja/items/{id}"),
        format!("/en/items/{id}"),
    ]
    .iter()
    .any(|expected| path == expected.as_str())
        && safety::item_id(raw).as_deref() == Some(id)
}
pub(super) fn diagnose_identity(html: &str, id: &str) -> Result<IdentityDiagnostics> {
    let expected = safety::free_item_route(id)?;
    let doc = Html::parse_document(html);
    if doc
        .select(&sel(
            "input[type='password'],form[action*='sign_in'],form[action*='login']",
        ))
        .next()
        .is_some()
    {
        bail!("AUTH_REQUIRED");
    }
    if doc
        .select(&sel("#challenge-form,.cf-challenge,iframe[src*='captcha']"))
        .next()
        .is_some()
    {
        bail!("AUTH_CHALLENGE");
    }
    // Attribute/count inspection only: no filename selectors, text(), or production file parsing.
    let canonical: Vec<_> = doc.select(&sel("link[rel='canonical']")).collect();
    let og: Vec<_> = doc.select(&sel("meta[property='og:url']")).collect();
    let roots: Vec<_> = doc
        .select(&sel("div#items.market[data-product-id]"))
        .collect();
    let root_match = roots.len() == 1 && roots[0].value().attr("data-product-id") == Some(id);
    let mut counts = IdentityDiagnostics {
        version: 1,
        canonical_count: canonical.len(),
        og_count: og.len(),
        root_count: roots.len(),
        list_count: doc.select(&sel("ul#variations")).count(),
        root_id_match: root_match,
        lists_within_matching_root: if root_match {
            roots[0].select(&sel("ul#variations")).count()
        } else {
            0
        },
        ..IdentityDiagnostics::default()
    };
    let classify = |raw: Option<&str>| {
        let Some(raw) = raw else {
            return (false, false, false, false, false, true);
        };
        let ja = raw == expected;
        let en = raw == format!("https://booth.pm/en/items/{id}");
        let bare = raw == format!("https://booth.pm/items/{id}")
            && safety::observed_product_url(raw).as_deref() == Some(expected.as_str());
        let shop = shop_item_identity(raw, id);
        (ja, bare, ja, en, shop, !(ja || bare || en || shop))
    };
    if canonical.len() == 1 {
        (
            counts.canonical_exact,
            counts.canonical_old_bare_same_id,
            counts.canonical_locale_ja_same_id,
            counts.canonical_locale_en_same_id,
            counts.canonical_shop_host_same_id,
            counts.canonical_other_shape,
        ) = classify(canonical[0].value().attr("href"));
    }
    if og.len() == 1 {
        (
            counts.og_exact,
            counts.og_old_bare_same_id,
            counts.og_locale_ja_same_id,
            counts.og_locale_en_same_id,
            counts.og_shop_host_same_id,
            counts.og_other_shape,
        ) = classify(og[0].value().attr("content"));
    }
    Ok(counts)
}
fn pair_name<'a>(scope: ElementRef<'a>, button: ElementRef<'a>) -> Option<ElementRef<'a>> {
    let raw: Vec<_> = scope.select(&sel(CONTROLS)).collect();
    let names: Vec<_> = scope.select(&sel(NAMES)).collect();
    if raw.len() != 1 || control(raw[0]).is_none() || names.len() != 1 {
        return None;
    }
    let name = text(names[0]);
    (clean_name(&name) && button.value().attr("title").map(str::trim) == Some(name.as_str()))
        .then(|| names[0])
}
fn cart_wrap<'a>(node: ElementRef<'a>, row: ElementRef<'a>) -> Option<ElementRef<'a>> {
    node.ancestors()
        .filter_map(ElementRef::wrap)
        .take_while(|n| n.id() != row.id())
        .find(|n| n.value().classes().any(|c| c == "cart-button-wrap"))
}
pub(super) fn diagnose_entry(html: &str, id: &str) -> Result<EntryDiagnostics> {
    // The production parser's unchanged identity/list gates run before any counts.
    let evidence = parse(html, id)?;
    if matches!(
        evidence.error_code.as_deref(),
        Some("FREE_ITEM_IDENTITY_UNPROVEN" | "FREE_ITEM_LIST_UNPROVEN")
    ) {
        bail!("FREE_ITEM_IDENTITY_UNPROVEN");
    }
    let doc = Html::parse_document(html);
    let root = doc
        .select(&sel("div#items.market[data-product-id]"))
        .next()
        .ok_or_else(|| anyhow::anyhow!("FREE_ITEM_IDENTITY_UNPROVEN"))?;
    let list = root
        .select(&sel("ul#variations"))
        .next()
        .ok_or_else(|| anyhow::anyhow!("FREE_ITEM_IDENTITY_UNPROVEN"))?;
    let root_controls: Vec<_> = root.select(&sel(CONTROLS)).collect();
    let list_controls: Vec<_> = list.select(&sel(CONTROLS)).collect();
    let mut counts = EntryDiagnostics {
        version: 1,
        identity_bound: true,
        entry_still_unproven: evidence.error_code.as_deref() == Some("FREE_ITEM_ENTRY_UNPROVEN"),
        current_parser_complete: evidence.manifest_complete,
        raw_controls: root_controls.len(),
        orphan_controls: root_controls.len().saturating_sub(list_controls.len()),
        ..EntryDiagnostics::default()
    };
    // Structural observations only. No marker or price proves owned-file
    // unavailability; the production role predicate also requires a bound list.
    let paid_role =
        sel("form button.add-cart[data-product-id],form button.add-gift[data-product-id]");
    let disabled = sel("button[disabled],input[disabled],button[aria-disabled='true'],input[aria-disabled='true'],a[aria-disabled='true']");
    let unavailable = sel("[data-test='item-unavailable'],.item-unavailable");
    let controls = sel(CONTROLS);
    let forms = sel("form");
    let variation_inputs = sel("input[name='cart_item[variation_id]']");
    let disabled_cart = sel(PAUSED_CART);
    let restock_notice = sel(RESTOCK_NOTICE);
    for row in list.children().filter_map(ElementRef::wrap) {
        counts.variation_rows += 1;
        let has_control = row.select(&controls).next().is_some();
        let has_paid_role = non_free_role(row, id);
        counts.recognized_non_free_rows += usize::from(!has_control && has_paid_role);
        counts.unrecognized_role_rows += usize::from(!has_control && !has_paid_role);
        counts.static_disabled_rows += usize::from(row.select(&disabled).next().is_some());
        counts.unavailable_marker_rows += usize::from(row.select(&unavailable).next().is_some());
        let positive_price = positive_row_price(row);
        counts.positive_price_rows += usize::from(positive_price);
        counts.bound_paid_variation_rows += usize::from(
            positive_price
                && row.select(&forms).any(|form| {
                    let inputs: Vec<_> = form.select(&variation_inputs).collect();
                    inputs.len() == 1
                        && inputs[0].value().attr("value").is_some_and(safety::numeric)
                        && form
                            .select(&paid_role)
                            .any(|button| button.value().attr("data-product-id") == Some(id))
                }),
        );
        counts.disabled_cart_rows += usize::from(row.select(&disabled_cart).next().is_some());
        counts.restock_notice_rows += usize::from(row.select(&restock_notice).next().is_some());
    }
    let mut seen_wraps = vec![];
    let mut matched_names = vec![];
    for button in list_controls {
        if control(button).is_none() {
            continue;
        }
        counts.valid_controls += 1;
        let labels: Vec<_> = button.select(&sel(".cmd-label")).collect();
        if labels.len() != 1 || text(labels[0]) != "無料ダウンロード" {
            continue;
        }
        counts.free_controls += 1;
        let row = button
            .ancestors()
            .filter_map(ElementRef::wrap)
            .take_while(|n| n.id() != list.id())
            .find(|n| {
                n.value().name() == "li" && n.value().classes().any(|c| c == "variation-item")
            });
        let Some(row) = row else {
            counts.orphan_controls += 1;
            continue;
        };
        let scope = button
            .ancestors()
            .filter_map(ElementRef::wrap)
            .take_while(|n| n.id() != row.id())
            .find(|n| n.value().classes().any(|c| c == "cart-button-wrap"));
        let Some(scope) = scope else {
            counts.controls_missing_cart_wrap += 1;
            continue;
        };
        if !seen_wraps.contains(&scope.id()) {
            seen_wraps.push(scope.id());
            counts.wraps_with_multiple_valid_controls += usize::from(
                scope
                    .select(&sel(CONTROLS))
                    .filter(|n| control(*n).is_some())
                    .count()
                    > 1,
            );
            let names = scope.select(&sel(NAMES)).count();
            counts.wraps_without_filename += usize::from(names == 0);
            counts.wraps_with_multiple_filenames += usize::from(names > 1);
        }
        let title = button.value().attr("title").map(str::trim);
        let agreeing: Vec<_> = scope
            .select(&sel(NAMES))
            .filter(|n| {
                let name = text(*n);
                clean_name(&name) && title == Some(name.as_str())
            })
            .collect();
        counts.title_agreements += usize::from(agreeing.len() == 1);
        if let Some(name) = pair_name(scope, button) {
            if !matched_names.contains(&name.id()) {
                matched_names.push(name.id());
            }
        }
        let mut first_depth = None;
        // ego-tree ancestors() starts at the parent, so keep the first inner scope.
        for (index, inner) in button
            .ancestors()
            .filter_map(ElementRef::wrap)
            .take_while(|n| n.id() != scope.id())
            .take(8)
            .enumerate()
        {
            if let Some(name) = pair_name(inner, button) {
                first_depth = Some(index + 1);
                if !matched_names.contains(&name.id()) {
                    matched_names.push(name.id());
                }
                break;
            }
        }
        counts.inner_single_control_single_filename_scopes_within4_ancestors +=
            usize::from(first_depth.is_some_and(|d| d <= 4));
        counts.inner_single_control_single_filename_scopes_within8_ancestors +=
            usize::from(first_depth.is_some());
    }
    counts.distinct_matched_filename_nodes = matched_names.len();
    counts.orphan_names = root
        .select(&sel(NAMES))
        .filter(|n| !matched_names.contains(&n.id()))
        .count();
    for node in root.select(&sel(
        "[data-dropdown-items],[data-test='other-downloads-button']",
    )) {
        let mut known = false;
        for scope in node
            .ancestors()
            .filter_map(ElementRef::wrap)
            .take_while(|n| n.id() != root.id())
            .take(8)
        {
            let controls: Vec<_> = scope.select(&sel(CONTROLS)).collect();
            if controls.len() == 1 {
                if let Some((file, variant, _)) = control(controls[0]) {
                    if pair_name(scope, controls[0])
                        .is_some_and(|name| alternate(node, &file, &variant, &text(name)))
                    {
                        known = true;
                        break;
                    }
                }
            }
        }
        counts.known_alternate_methods += usize::from(known);
        counts.unknown_dropdowns += usize::from(!known);
    }
    counts.expansion_hints=root.select(&sel("[data-test='load-more'],[data-test='show-more'],[data-test='more-downloadables'],[aria-expanded='false']:not([data-test='other-downloads-button']),select")).count();
    Ok(counts)
}
pub(super) fn parse(html: &str, id: &str) -> Result<Evidence> {
    safety::free_item_route(id)?;
    let doc = Html::parse_document(html);
    if doc
        .select(&sel(
            "input[type='password'],form[action*='sign_in'],form[action*='login']",
        ))
        .next()
        .is_some()
    {
        bail!("AUTH_REQUIRED");
    }
    if doc
        .select(&sel("#challenge-form,.cf-challenge,iframe[src*='captcha']"))
        .next()
        .is_some()
    {
        bail!("AUTH_CHALLENGE");
    }
    let unknown = |code| Evidence::unknown(id, code, false);
    let canonical: Vec<_> = doc.select(&sel("link[rel='canonical']")).collect();
    let og: Vec<_> = doc.select(&sel("meta[property='og:url']")).collect();
    let roots: Vec<_> = doc
        .select(&sel("div#items.market[data-product-id]"))
        .collect();
    // Canonical and OG may name the item by any HTTPS BOOTH URL: a locale prefix or the
    // shop's own host. The root and every list row must still carry this item's ID.
    let same_item = |raw: Option<&str>| raw.and_then(safety::item_id).as_deref() == Some(id);
    if canonical.len() != 1
        || !same_item(canonical[0].value().attr("href"))
        || og.len() != 1
        || !same_item(og[0].value().attr("content"))
        || roots.len() != 1
        || roots[0].value().attr("data-product-id") != Some(id)
    {
        return Ok(unknown("FREE_ITEM_IDENTITY_UNPROVEN"));
    }
    let root = roots[0];
    let lists: Vec<_> = doc.select(&sel("ul#variations")).collect();
    if lists.len() != 1 || !root.descendants().any(|n| n.id() == lists[0].id()) {
        return Ok(unknown("FREE_ITEM_LIST_UNPROVEN"));
    }
    let list = lists[0];
    if std::iter::once(list)
        .chain(list.select(&sel("[data-product-id],[data-item-id]")))
        .any(|n| {
            ["data-product-id", "data-item-id"]
                .iter()
                .any(|attr| n.value().attr(attr).is_some_and(|v| v != id))
        })
    {
        return Ok(unknown("FREE_ITEM_IDENTITY_UNPROVEN"));
    }
    let rows: Vec<_> = list.children().filter_map(ElementRef::wrap).collect();
    if rows.is_empty()
        || rows.iter().any(|n| {
            n.value().name() != "li" || !n.value().classes().any(|c| c == "variation-item")
        })
        || list.select(&sel("li.variation-item")).count() != rows.len()
    {
        return Ok(unknown("FREE_ITEM_LIST_UNPROVEN"));
    }
    if root.select(&sel("[data-test='load-more'],[data-test='show-more'],[data-test='more-downloadables'],button[aria-expanded='false']:not([data-test='other-downloads-button']),[data-test='variation-select'],[data-test='variation-selector'],select[name='variation_id'],select[name='variation']")).next().is_some() { return Ok(unknown("FREE_ITEM_DYNAMIC_LIST")); }
    if list
        .select(&sel(
            "select,[aria-expanded='false']:not([data-test='other-downloads-button'])",
        ))
        .next()
        .is_some()
    {
        return Ok(unknown("FREE_ITEM_DYNAMIC_LIST"));
    }
    let root_controls: Vec<_> = root.select(&sel(CONTROLS)).collect();
    let list_controls: Vec<_> = list.select(&sel(CONTROLS)).collect();
    if root_controls.len() != list_controls.len() {
        return Ok(unknown("FREE_ITEM_ORPHAN_CONTROL"));
    }
    let mut files = BTreeMap::new();
    let mut accepted_names = vec![];
    let mut accepted_labels = vec![];
    let mut accepted_alternates = vec![];
    for row in &rows {
        if row.select(&sel(CONTROLS)).next().is_none() && !non_free_role(*row, id) {
            return Ok(unknown("FREE_ITEM_ROLE_UNPROVEN"));
        }
        let mut processed_wraps = vec![];
        for outer_button in row.select(&sel(CONTROLS)) {
            let Some(scope) = cart_wrap(outer_button, *row) else {
                return Ok(unknown("FREE_ITEM_ENTRY_UNPROVEN"));
            };
            if processed_wraps.contains(&scope.id()) {
                continue;
            }
            processed_wraps.push(scope.id());
            let controls: Vec<_> = scope.select(&sel(CONTROLS)).collect();
            let names: Vec<_> = scope.select(&sel(NAMES)).collect();
            if controls.is_empty()
                || controls.len() != names.len()
                || controls
                    .iter()
                    .chain(names.iter())
                    .any(|node| cart_wrap(*node, *row).map(|wrap| wrap.id()) != Some(scope.id()))
                || scope
                    .select(&sel(".cart-button-wrap,button.add-cart,button.add-gift"))
                    .next()
                    .is_some()
            {
                return Ok(unknown("FREE_ITEM_ENTRY_UNPROVEN"));
            }
            let mut names_by_text = BTreeMap::new();
            for node in names {
                let name = text(node);
                if !clean_name(&name) || names_by_text.insert(name, node).is_some() {
                    return Ok(unknown("FREE_ITEM_FILENAME_UNPROVEN"));
                }
            }
            let mut consumed_names = vec![];
            let mut wrap_keys = vec![];
            for button in controls {
                let Some((file_id, variant_id, source_url)) = control(button) else {
                    return Ok(unknown("FREE_ITEM_INVALID_CONTROL"));
                };
                if row
                    .value()
                    .attr("data-variation-id")
                    .is_some_and(|v| v != variant_id)
                {
                    return Ok(unknown("FREE_ITEM_IDENTITY_UNPROVEN"));
                }
                let Some(name) = button.value().attr("title").map(str::trim) else {
                    return Ok(unknown("FREE_ITEM_FILENAME_UNPROVEN"));
                };
                let Some(node) = names_by_text.get(name) else {
                    return Ok(unknown("FREE_ITEM_FILENAME_UNPROVEN"));
                };
                if consumed_names.contains(&node.id()) {
                    return Ok(unknown("FREE_ITEM_FILENAME_UNPROVEN"));
                }
                consumed_names.push(node.id());
                let key = (file_id.clone(), variant_id.clone());
                if files.contains_key(&key) {
                    return Ok(unknown("FREE_ITEM_CONFLICTING_ENTRIES"));
                }
                accepted_names.push(node.id());
                // Exact product/variant/resolver and filename binding prove this
                // offered file. Button wording and optional labels are display UI.
                accepted_labels.extend(button.select(&sel(".cmd-label")).map(|label| label.id()));
                wrap_keys.push((key.clone(), name));
                files.insert(
                    key,
                    File {
                        id: file_id,
                        variant_id: Some(variant_id),
                        name: Some(name.to_owned()),
                        size_bytes: None,
                        source_url,
                        revision: "unknown".into(),
                        format: None,
                    },
                );
            }
            if consumed_names.len() != names_by_text.len() {
                return Ok(unknown("FREE_ITEM_FILENAME_UNPROVEN"));
            }
            for other in scope.select(&sel(
                "[data-dropdown-items],[data-test='other-downloads-button']",
            )) {
                if cart_wrap(other, *row).map(|wrap| wrap.id()) != Some(scope.id())
                    || wrap_keys
                        .iter()
                        .filter(|((file, variant), name)| alternate(other, file, variant, name))
                        .count()
                        != 1
                {
                    return Ok(unknown("FREE_ITEM_UNKNOWN_DROPDOWN"));
                }
                accepted_alternates.push(other.id());
            }
        }
    }
    if root
        .select(&sel(NAMES))
        .any(|n| !accepted_names.contains(&n.id()))
        || root
            .select(&sel(".cmd-label"))
            .any(|n| text(n) == "無料ダウンロード" && !accepted_labels.contains(&n.id()))
    {
        return Ok(unknown("FREE_ITEM_ORPHAN_ENTRY"));
    }
    if root
        .select(&sel(
            "[data-dropdown-items],[data-test='other-downloads-button']",
        ))
        .any(|n| !accepted_alternates.contains(&n.id()))
    {
        return Ok(unknown("FREE_ITEM_UNKNOWN_DROPDOWN"));
    }
    for node in std::iter::once(list).chain(list.select(&sel(
        "[data-file-count],[data-downloadable-count],[data-total-count]",
    ))) {
        let mut keys = BTreeSet::new();
        for button in node.select(&sel(CONTROLS)) {
            let Some((file, variant, _)) = control(button) else {
                return Ok(unknown("FREE_ITEM_INVALID_CONTROL"));
            };
            keys.insert((file, variant));
        }
        for attr in [
            "data-file-count",
            "data-downloadable-count",
            "data-total-count",
        ] {
            if node
                .value()
                .attr(attr)
                .is_some_and(|v| v.parse::<usize>().ok() != Some(keys.len()))
            {
                return Ok(unknown("FREE_ITEM_COUNT_UNPROVEN"));
            }
        }
    }
    if files.is_empty() {
        // This settles only the current public free offer. It never certifies
        // owned-file membership or revokes a previously observed entitlement.
        if root_controls.is_empty()
            && rows
                .iter()
                .all(|row| positive_row_price(*row) && non_free_role(*row, id))
        {
            return Ok(Evidence::unknown(id, "FREE_OFFER_UNAVAILABLE", true));
        }
        return Ok(unknown("FREE_FILE_CONTROLS_UNAVAILABLE"));
    }
    Ok(Evidence {
        product_id: id.into(),
        files: files.into_values().collect(),
        manifest_complete: true,
        unavailable: false,
        error_code: None,
    })
}

#[cfg(test)]
#[path = "free_item_tests.rs"]
mod tests;
