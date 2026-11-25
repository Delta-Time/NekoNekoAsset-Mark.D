//! Which listing and detail pages a scan reads next.
use super::*;

pub(super) fn selected_purchase_routes(
    acquisitions: &[Value],
    remote: &[Value],
    account: &str,
    selected: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut routes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for product in selected {
        let mut found = BTreeSet::new();
        for acquisition in acquisitions.iter().filter(|a| {
            a["accountId"] == account
                && a["source"] == "purchase"
                && (a["productId"] == *product || a["observedProductId"] == *product)
        }) {
            if let Some(order) = acquisition["orderId"]
                .as_str()
                .filter(|id| safety::numeric(id))
            {
                found.insert(format!("/orders/{order}"));
            }
        }
        for file in remote.iter().filter(|file| {
            file["accountId"] == account
                && file["source"] == "purchase"
                && file["productId"] == *product
        }) {
            if let Some(route) = file["resolverPage"]
                .as_str()
                .and_then(|route| safety::route(route, "purchase").ok())
            {
                if route.query().is_none()
                    && route
                        .path()
                        .strip_prefix("/orders/")
                        .is_some_and(safety::numeric)
                {
                    found.insert(route.path().to_owned());
                }
            }
        }
        for route in found {
            routes.entry(route).or_default().insert(product.clone());
        }
    }
    routes
}

/// Gives up on stopping early: the plain pages go on from where they stopped.
pub(super) fn read_every_page(checkpoint: &mut Checkpoint) {
    let read = checkpoint
        .committed
        .keys()
        .filter_map(|route| route.strip_prefix("/library?page="))
        .filter_map(|page| page.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    checkpoint.read_every_page = true;
    checkpoint.overlap_end = None;
    checkpoint.terminal = false;
    checkpoint.pending = vec![format!("/library?page={}", read + 1)];
}

pub(super) fn limit_listing_pages(checkpoint: &mut Checkpoint, limit: usize) {
    if checkpoint.pages >= limit {
        let before = checkpoint.pending.len();
        // Drain receipts/item endpoints and their metadata without following
        // another listing page or pretending the source reached its end.
        checkpoint.pending.retain(|route| detail_route(route));
        if checkpoint.pending.len() != before {
            checkpoint.terminal = false;
            checkpoint.unresolved += 1;
        }
    }
}

pub(super) fn detail_route(route: &str) -> bool {
    safety::free_item_id(route).is_some()
        || route.strip_prefix("/orders/").is_some_and(safety::numeric)
}

#[cfg(test)]
pub(super) fn next_routes(pending: &[String]) -> Result<Vec<String>> {
    available_routes(pending, &BTreeSet::new())
}

pub(super) fn available_routes(
    pending: &[String],
    active: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let listing: Vec<_> = pending
        .iter()
        .filter(|r| !detail_route(r) && !active.contains(*r))
        .cloned()
        .collect();
    let details: Vec<_> = pending
        .iter()
        .filter(|r| detail_route(r) && !active.contains(*r))
        .take(2usize.saturating_sub(active.iter().filter(|r| detail_route(r)).count()))
        .cloned()
        .collect();
    let mut routes = Vec::new();
    if !listing.is_empty() && !active.iter().any(|r| !detail_route(r)) {
        routes.push(next_route(&listing)?);
    }
    routes.extend(details);
    if routes.is_empty() && active.is_empty() {
        bail!("CHECKPOINT_INVALID");
    }
    Ok(routes)
}

pub(super) fn next_route(pending: &[String]) -> Result<String> {
    pending
        .iter()
        .min_by_key(|route| {
            if route.contains('?') {
                route
                    .rsplit('=')
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(u32::MAX)
            } else {
                u32::MAX
            }
        })
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))
}

pub(super) fn overlap_changed(
    c: &Checkpoint,
    head: &parser::Page,
    boundary: Option<&parser::Page>,
) -> bool {
    if head.fingerprint != c.head {
        return true;
    }
    if let Some(route) = c.boundary.as_deref().filter(|r| *r != head.route) {
        return boundary
            .is_none_or(|p| p.route != route || c.committed.get(route) != Some(&p.fingerprint));
    }
    false
}
