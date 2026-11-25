use super::*;
use rusqlite::types::Value as Sql;
use std::collections::{hash_map::DefaultHasher, BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

/// List facts reused until the trigger-maintained products revision changes.
/// Every list column follows the file-heavy body, so filters, sorts, search,
/// counts and list summaries read a compact per-connection TEMP copy instead of
/// each stored row; the library schema is unchanged. TEMP triggers record the
/// rows this connection changes, so a refresh rereads only those rows. Summary
/// fingerprints let a client skip summaries it already holds.
#[derive(Default)]
pub(super) struct ListSnapshot {
    revision: Option<i64>,
    total: i64,
    /// Sidebar counts by which products the list hides (`hidden_set`), then by whether
    /// Unclassified products are hidden from All.
    counts: [[api::ListCounts; 2]; 4],
    summaries: HashMap<String, u64>,
}

/// The count set for what the list hides: unavailable products, terms-only ones, or both.
fn hidden_set(unavailable: bool, terms_only: bool) -> usize {
    usize::from(unavailable) | usize::from(terms_only) << 1
}

/// A store product that has no bytes on disk and whose store lists no file for it, or only
/// files whose free offer ended, cannot be downloaded. Derived from stored rows, so none is
/// rewritten.
macro_rules! unavailable_sql {
    () => {
        "(COALESCE(json_extract(p.summary,'$.provider'),'') IN('booth','gumroad','jinxxy') AND NOT EXISTS(SELECT 1 FROM entities x WHERE x.kind='artifact' AND x.product_id=p.id AND (json_extract(x.body,'$.originalAvailable')=1 OR json_extract(x.body,'$.extractedPath') IS NOT NULL)) AND COALESCE(json_extract(p.summary,'$.endedFiles'),0)=COALESCE(json_extract(p.summary,'$.total'),1))"
    };
}
/// How acquisition row `a` was obtained. An account that received the product as a gift
/// holds it only as a gift, so its other rows, such as a free variant, say nothing. An
/// account that bought it holds it as bought, so its free-download row, such as a 0-yen
/// license file, says nothing. A row with no proof of payment says nothing when another
/// row of that account has one, and otherwise follows that account's current price.
const ACQUISITION_KIND: &str = "CASE WHEN json_extract(a.value,'$.source')='gift' THEN 'gift' WHEN EXISTS(SELECT 1 FROM json_each(p.provenance) g WHERE json_extract(g.value,'$.accountId') IS json_extract(a.value,'$.accountId') AND (json_extract(g.value,'$.source')='gift' OR (json_extract(a.value,'$.source')='free' AND json_extract(g.value,'$.source') IN('library','purchase')))) THEN '' WHEN json_extract(a.value,'$.source')='free' OR json_extract(a.value,'$.paidPrice')=0 THEN 'free' WHEN json_extract(a.value,'$.paidPrice')>0 THEN 'paid' WHEN EXISTS(SELECT 1 FROM json_each(p.provenance) o WHERE json_extract(o.value,'$.accountId') IS json_extract(a.value,'$.accountId') AND json_extract(o.value,'$.paidPrice') IS NOT NULL) THEN '' WHEN COALESCE((SELECT MAX(json_extract(v.value,'$.price')) FROM json_each(p.prices) v,json_each(v.value,'$.accountIds') x WHERE x.value=json_extract(a.value,'$.accountId')),p.price)=0 THEN 'free' ELSE 'paid' END";
// Keep the effective values of the former summary predicates and ordering.
const LIST_COLUMNS: &str =concat!("id,category,status,sensitive,downloaded,title,acquired,price,search,in_all,COALESCE(json_extract(summary,'$.hidden'),0),COALESCE(json_extract(summary,'$.priceCurrency'),'JPY'),COALESCE(json_extract(summary,'$.provider'),'')='local',json_extract(summary,'$.accountIds'),json_extract(summary,'$.ownedAvatarEvidence'),json_extract(summary,'$.provenance'),json_extract(summary,'$.ownedCurrentPrices'),json_extract(summary,'$.newlyDiscovered'),", unavailable_sql!(), ",0,summary");
/// The rows that may hold only terms of use (`terms_only`): BOOTH products every row of
/// which is a free download, with few files. Only these read their files and listing.
const TERMS_CANDIDATES: &str = "json_extract(l.summary,'$.provider')='booth' AND COALESCE(json_extract(l.summary,'$.total'),0) BETWEEN 1 AND 20 AND EXISTS(SELECT 1 FROM json_each(l.provenance)) AND NOT EXISTS(SELECT 1 FROM json_each(l.provenance) a WHERE json_extract(a.value,'$.source') IS NOT 'free')";
// The UPDATE condition matches products_changed_update, so each recorded event is one revision.
// Trigger steps inherit an upsert's conflict policy, so the change log has no key and may repeat ids.
const LIST_SCHEMA: &str = "CREATE TEMP TABLE IF NOT EXISTS list_rows(id TEXT PRIMARY KEY,category TEXT NOT NULL,status TEXT NOT NULL,sensitive INTEGER NOT NULL,downloaded INTEGER NOT NULL,title TEXT NOT NULL,acquired TEXT,price REAL,search TEXT NOT NULL,in_all INTEGER NOT NULL,hidden,currency,local INTEGER NOT NULL,accounts,evidence,provenance,prices,newly,unavailable INTEGER NOT NULL,terms_only INTEGER NOT NULL,summary TEXT NOT NULL);
 CREATE TEMP TABLE IF NOT EXISTS list_changes(id);
 CREATE TEMP TABLE IF NOT EXISTS list_events(n INTEGER NOT NULL);
 INSERT INTO temp.list_events SELECT 0 WHERE NOT EXISTS(SELECT 1 FROM temp.list_events);
 CREATE TEMP TRIGGER IF NOT EXISTS list_insert AFTER INSERT ON main.products BEGIN INSERT INTO list_changes VALUES(NEW.id); UPDATE list_events SET n=n+1; END;
 CREATE TEMP TRIGGER IF NOT EXISTS list_update AFTER UPDATE ON main.products WHEN NEW.summary<>OLD.summary OR NEW.search<>OLD.search BEGIN INSERT INTO list_changes VALUES(OLD.id),(NEW.id); UPDATE list_events SET n=n+1; END;
 CREATE TEMP TRIGGER IF NOT EXISTS list_delete AFTER DELETE ON main.products BEGIN INSERT INTO list_changes VALUES(OLD.id); UPDATE list_events SET n=n+1; END;";

fn snapshot<'a>(conn: &Connection, cache: &'a mut ListSnapshot) -> Result<&'a ListSnapshot> {
    let revision: i64 = conn.query_row(
        "SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM meta WHERE key='products_revision'),0)",
        [],
        |row| row.get(0),
    )?;
    if cache.revision == Some(revision) {
        return Ok(cache);
    }
    // An enclosing transaction could roll the copy back behind this cache.
    ensure!(conn.is_autocommit(), "catalogue.list_snapshot_transaction");
    let previous = cache.revision.take();
    conn.execute_batch(LIST_SCHEMA)?;
    conn.execute_batch("SAVEPOINT list_rows;")?;
    let filled = (|| -> Result<ListSnapshot> {
        // Reread only the recorded rows when they account for every revision since the copy.
        let events: i64 = conn.query_row("SELECT n FROM temp.list_events", [], |row| row.get(0))?;
        let partial = previous.is_some_and(|old| revision - old == events);
        let mut next = ListSnapshot {
            revision: Some(revision),
            summaries: if partial {
                std::mem::take(&mut cache.summaries)
            } else {
                HashMap::new()
            },
            ..Default::default()
        };
        let changed = if partial {
            let mut stmt = conn.prepare("SELECT DISTINCT id FROM temp.list_changes")?;
            for id in stmt.query_map([], |row| row.get::<_, String>(0))? {
                next.summaries.remove(&id?);
            }
            "WHERE id IN(SELECT id FROM temp.list_changes)"
        } else {
            ""
        };
        conn.execute(&format!("DELETE FROM temp.list_rows {changed}"), [])?;
        conn.execute(
            &format!("INSERT INTO temp.list_rows SELECT {LIST_COLUMNS} FROM products p {changed}"),
            [],
        )?;
        mark_terms_only(conn, partial)?;
        let mut stmt = conn.prepare(&format!("SELECT id,summary FROM temp.list_rows {changed}"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let mut hasher = DefaultHasher::new();
            row.get_ref(1)?.as_str()?.hash(&mut hasher);
            next.summaries.insert(row.get(0)?, hasher.finish());
        }
        conn.execute_batch("DELETE FROM temp.list_changes; UPDATE temp.list_events SET n=0;")?;
        // One count set for each combination of what the list hides (`hidden_set`).
        let mut sets: [(BTreeMap<String, i64>, BTreeMap<String, i64>, i64, i64); 4] =
            Default::default();
        let mut hidden = 0i64;
        let mut stmt = conn.prepare("SELECT category,status,in_all=1,downloaded=1,hidden=0,hidden=1,unavailable=1,terms_only=1,COUNT(*) FROM temp.list_rows GROUP BY 1,2,3,4,5,6,7,8")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let count: i64 = row.get(8)?;
            next.total += count;
            if row.get(4)? {
                let (unavailable, terms_only): (bool, bool) = (row.get(6)?, row.get(7)?);
                for (index, set) in sets.iter_mut().enumerate() {
                    if (index & 1 == 1 && unavailable) || (index & 2 == 2 && terms_only) {
                        continue;
                    }
                    *set.0.entry(row.get(0)?).or_default() += count;
                    *set.1.entry(row.get(1)?).or_default() += count;
                    set.2 += count * i64::from(row.get::<_, bool>(2)?);
                    set.3 += count * i64::from(row.get::<_, bool>(3)?);
                }
            }
            hidden += count * i64::from(row.get::<_, bool>(5)?);
        }
        next.counts = sets.map(|(categories, statuses, all, downloaded)| {
            let attention = statuses.get("attention").copied().unwrap_or(0);
            // All hides Unclassified products unless the user lists them there.
            let listed: i64 = statuses.values().sum();
            [listed, all].map(|all| api::ListCounts {
                all,
                downloaded,
                attention,
                hidden: Some(hidden),
                categories: Some(categories.clone()),
                statuses: Some(statuses.clone()),
            })
        });
        Ok(next)
    })();
    match filled {
        Ok(next) => {
            conn.execute_batch("RELEASE list_rows;")?;
            *cache = next;
            Ok(cache)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO list_rows; RELEASE list_rows;");
            Err(error)
        }
    }
}

/// Whether a candidate's files are only its listing's terms of use.
fn files_terms_only(product: &Value, source: &Value) -> bool {
    let files = product["files"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    // A file the user added is theirs, whatever it is called.
    let listed: Option<Vec<_>> = files
        .iter()
        .map(|file| {
            (file["userAdded"] != true).then(|| super::terms_only::TermsFile {
                name: file["name"].as_str().unwrap_or(""),
                variant_id: file["provenance"][0]["variantId"].as_str(),
            })
        })
        .collect();
    listed.is_some_and(|files| super::terms_only::terms_only(&files, &source["publicVariants"]))
}

/// Whether the list leaves this product out as only a listing's terms of use while that
/// preference is on: `TERMS_CANDIDATES`, then the files, as `mark_terms_only` reads them.
pub(super) fn terms_only_product(conn: &Connection, id: &str) -> Result<bool> {
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT p.summary,p.body,s.body FROM products p LEFT JOIN entities s ON s.kind='product' AND s.id=p.id WHERE p.id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((summary, body, source)) = row else {
        return Ok(false);
    };
    let summary: Value = serde_json::from_str(&summary)?;
    let candidate = summary["provider"] == "booth"
        && (1.0..=20.0).contains(&summary["total"].as_f64().unwrap_or(0.0))
        && summary["provenance"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["source"] == "free"));
    if !candidate {
        return Ok(false);
    }
    let product: Value = serde_json::from_str(&body)?;
    let source = source
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or(Value::Null);
    Ok(files_terms_only(&product, &source))
}

/// Marks the copied rows that hold only a listing's terms of use, from each candidate's files
/// and its listing's variations; a partial refresh reads only the rows it copied again.
fn mark_terms_only(conn: &Connection, partial: bool) -> Result<()> {
    let changed = if partial {
        " AND l.id IN(SELECT id FROM temp.list_changes)"
    } else {
        ""
    };
    let mut stmt = conn.prepare(&format!("SELECT l.id,p.body,s.body FROM temp.list_rows l JOIN products p ON p.id=l.id LEFT JOIN entities s ON s.kind='product' AND s.id=l.id WHERE {TERMS_CANDIDATES}{changed}"))?;
    let mut rows = stmt.query([])?;
    let mut marked = vec![];
    while let Some(row) = rows.next()? {
        let product: Value = serde_json::from_str(row.get_ref(1)?.as_str()?)?;
        let source: Value = row
            .get::<_, Option<String>>(2)?
            .and_then(|body| serde_json::from_str(&body).ok())
            .unwrap_or(Value::Null);
        if files_terms_only(&product, &source) {
            marked.push(row.get::<_, String>(0)?);
        }
    }
    let mut mark = conn.prepare("UPDATE temp.list_rows SET terms_only=1 WHERE id=?1")?;
    for id in marked {
        mark.execute([id])?;
    }
    Ok(())
}

pub(super) fn query(
    conn: &Connection,
    args: &api::QueryRequest,
    cache: &mut ListSnapshot,
) -> Result<Value> {
    let list = snapshot(conn, cache)?;
    let mut conditions = vec!["1=1".to_owned()];
    let hidden_only = args.hidden_only.unwrap_or(false);
    conditions.push(format!("p.hidden={}", i32::from(hidden_only)));
    // The Hidden scope shows every product the user hid, available or not.
    let hide_unavailable = args.hide_unavailable == Some(true) && !hidden_only;
    if hide_unavailable {
        conditions.push("p.unavailable=0".to_owned());
    }
    let hide_terms_only = args.hide_terms_only == Some(true) && !hidden_only;
    if hide_terms_only {
        conditions.push("p.terms_only=0".to_owned());
    }
    let hide_unclassified = args.hide_unclassified != Some(false);
    let mut binds: Vec<Sql> = vec![];
    let mut bind = |value: Sql| {
        binds.push(value);
        format!("?{}", binds.len())
    };
    let category = args.category.as_deref().unwrap_or("all");
    let quick = args.quick.as_deref().unwrap_or("all");
    let collection = args
        .collection_id
        .as_deref()
        .or(args.collection.as_deref())
        .unwrap_or("");
    if category != "all" && !category.is_empty() {
        conditions.push(format!(
            "p.category={}",
            bind(Sql::Text(category.to_owned()))
        ));
    } else if hide_unclassified && quick == "all" && collection.is_empty() && !hidden_only {
        conditions.push("p.in_all=1".to_owned());
    }
    if quick == "downloaded" {
        conditions.push("p.downloaded=1".to_owned());
    }
    if quick == "attention" {
        conditions.push("p.status='attention'".to_owned());
    }
    if quick == "new" {
        conditions.push("p.newly=1".to_owned());
    }
    if !collection.is_empty() {
        // Read membership once instead of reparsing the collection for each product.
        conditions.push(format!("p.id IN(SELECT m.value FROM entities c,json_each(c.body,'$.productIds') m WHERE c.kind='collection' AND c.id={})",bind(Sql::Text(collection.to_owned()))));
    }
    let sort = args.sort.as_deref().unwrap_or("acquired");
    let status = args.status.as_deref().unwrap_or("");
    if !status.is_empty() && status != "all" {
        conditions.push(format!("p.status={}", bind(Sql::Text(status.to_owned()))));
    }
    let sensitive = args
        .sensitive
        .as_deref()
        .or(args.sensitive_mode.as_deref())
        .or(args.r18.as_deref())
        .unwrap_or("show");
    match sensitive {
        "only" => conditions.push("p.sensitive=1".into()),
        "hide" => conditions.push("p.sensitive=0".into()),
        _ => {}
    }
    let account_ids = args.account_ids.as_ref();
    let include_offline = args.include_offline.unwrap_or(account_ids.is_none());
    let local_product = "p.local=1";
    if !include_offline {
        conditions.push(format!("NOT ({local_product})"));
    }
    let mut account_placeholders = vec![];
    if let Some(accounts) = account_ids {
        account_placeholders = accounts
            .iter()
            .map(|s| bind(Sql::Text(s.to_owned())))
            .collect::<Vec<_>>();
        let account_scope = if account_placeholders.is_empty() {
            "0=1".to_owned()
        } else {
            format!(
                "EXISTS(SELECT 1 FROM json_each(p.accounts) a WHERE a.value IN({}))",
                account_placeholders.join(",")
            )
        };
        conditions.push(if include_offline {
            format!("({local_product} OR {account_scope})")
        } else {
            account_scope
        });
    }
    if let Some(kinds) = &args.acquisition {
        conditions.push(acquisition_condition(
            kinds,
            account_ids,
            &account_placeholders,
            local_product,
        ));
    }
    // The license database chose these; an empty list keeps nothing.
    if let Some(ids) = &args.license_ids {
        conditions.push(format!(
            "p.id IN(SELECT value FROM json_each({}))",
            bind(Sql::Text(json!(ids).to_string()))
        ));
    }
    let avatar = args.avatar.as_deref().unwrap_or("");
    let mut pinned = vec![];
    if !avatar.is_empty() && avatar != "all" {
        avatar_filter(
            conn,
            avatar,
            hidden_only,
            sensitive,
            (hide_unavailable, hide_terms_only),
            account_ids,
            &account_placeholders,
            include_offline,
            local_product,
            &mut bind,
            &mut pinned,
            &mut conditions,
        )?;
    }
    let search = normalized(
        args.search
            .as_deref()
            .or(args.query.as_deref())
            .unwrap_or(""),
    );
    for term in search.split_whitespace() {
        let chars: Vec<_> = term.chars().collect();
        let gram: String = chars.iter().take(3).collect();
        conditions.push(format!(
            "p.id IN(SELECT product_id FROM search_grams WHERE gram={})",
            bind(Sql::Text(gram))
        ));
        conditions.push(format!(
            "instr(p.search,{})>0",
            bind(Sql::Text(term.to_owned()))
        ));
    }
    let sql = list_sql(
        args,
        sort,
        account_ids,
        &account_placeholders,
        local_product,
        &conditions,
    );
    let mut stmt = conn.prepare(&sql)?;
    let normal_ids = stmt
        .query_map(rusqlite::params_from_iter(binds.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // The explicit avatar Base exception overrides every filter and sort. Apply
    // before count/paging so pinned records occur exactly once on every page.
    let pinned_set: BTreeSet<_> = pinned.iter().cloned().collect();
    let mut all_downloaded = normal_ids.iter().all(|(_, downloaded)| *downloaded);
    for id in &pinned {
        all_downloaded &= conn.query_row(
            "SELECT downloaded=1 FROM temp.list_rows WHERE id=?1",
            [id],
            |row| row.get::<_, bool>(0),
        )?;
    }
    let mut ids = pinned;
    ids.extend(
        normal_ids
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !pinned_set.contains(id)),
    );
    all_downloaded &= !ids.is_empty();
    let (products, fingerprints) = page_products(conn, args, list, &ids, account_ids)?;
    // Sidebar counts follow the preferences in every scope, including Hidden.
    let counts = &list.counts[hidden_set(
        args.hide_unavailable == Some(true),
        args.hide_terms_only == Some(true),
    )][usize::from(hide_unclassified)];
    Ok(serde_json::to_value(api::QueryResult {
        products,
        total: list.total,
        matched: ids.len(),
        matched_ids: ids,
        counts: counts.clone(),
        all_downloaded,
        fingerprints,
    })?)
}

/// Products acquired as paid, free or gift, by the selected accounts when there are any.
fn acquisition_condition(
    kinds: &[String],
    account_ids: Option<&Vec<String>>,
    account_placeholders: &[String],
    local_product: &str,
) -> String {
    let listed = ["paid", "free", "gift"]
        .into_iter()
        .filter(|kind| kinds.iter().any(|value| value == kind))
        .map(|kind| format!("'{kind}'"))
        .collect::<Vec<_>>()
        .join(",");
    let scope = match account_ids {
        None => "1=1".to_owned(),
        Some(_) if account_placeholders.is_empty() => "0=1".to_owned(),
        Some(_) => format!(
            "json_extract(a.value,'$.accountId') IN({})",
            account_placeholders.join(",")
        ),
    };
    format!("{local_product}=0 AND (EXISTS(SELECT 1 FROM json_each(p.provenance) a WHERE {scope} AND {} IN({listed})) OR (NOT EXISTS(SELECT 1 FROM json_each(p.provenance) a WHERE {scope}) AND CASE WHEN p.price=0 THEN 'free' ELSE 'paid' END IN({listed})))", ACQUISITION_KIND)
}

/// A selected avatar keeps the products compatible with it and pins its own Base products.
fn avatar_filter(
    conn: &Connection,
    avatar: &str,
    hidden_only: bool,
    sensitive: &str,
    (hide_unavailable, hide_terms_only): (bool, bool),
    account_ids: Option<&Vec<String>>,
    account_placeholders: &[String],
    include_offline: bool,
    local_product: &str,
    bind: &mut impl FnMut(Sql) -> String,
    pinned: &mut Vec<String>,
    conditions: &mut Vec<String>,
) -> Result<()> {
    let preferences = raw(conn, "preferences", "app")?.unwrap_or(json!({}));
    let registry =
        avatars::registry_from(&projection::bases_in(conn, "temp.list_rows")?, &preferences)?;
    let avatar_id = registry.resolve(avatar);
    // Base pins override ordinary filters, but never the user's explicit
    // hidden/visible, R-18, unavailable or terms-only visibility boundaries.
    let mut visible_base = conn.prepare_cached(
            "SELECT hidden=?2 AND (?3 NOT IN('hide','only') OR sensitive=CASE ?3 WHEN 'only' THEN 1 ELSE 0 END) AND (?4=0 OR unavailable=0) AND (?5=0 OR terms_only=0) FROM temp.list_rows WHERE id=?1",
        )?;
    for id in registry.bases(&avatar_id) {
        if visible_base.query_row(
            params![
                id,
                hidden_only,
                sensitive,
                hide_unavailable,
                hide_terms_only
            ],
            |row| row.get::<_, bool>(0),
        )? {
            pinned.push(id);
        }
    }
    // Names and item links have no ID of this identity stored. Match the raw
    // evidence that option counts assign to it, before or after its aliases persist.
    let known = |id: &str| registry.entries.contains_key(id) || id.starts_with("avatar:");
    let mut pairs = conn.prepare_cached("SELECT DISTINCT COALESCE(json_extract(e.value,'$.avatarId'),''),COALESCE(json_extract(e.value,'$.avatar'),'') FROM temp.list_rows p,json_each(p.evidence) e")?;
    let mut rows = pairs.query([])?;
    let mut values = vec![];
    while let Some(row) = rows.next()? {
        let (evidence_id, name): (String, String) = (row.get(0)?, row.get(1)?);
        if registry
            .evidence_identity(&preferences, &evidence_id, &name, &known)
            .as_ref()
            == Some(&avatar_id)
        {
            values.push(format!(
                "({},{})",
                bind(Sql::Text(evidence_id)),
                bind(Sql::Text(name))
            ));
        }
    }
    let assigned = if values.is_empty() {
        "0=1".to_owned()
    } else {
        format!("(COALESCE(json_extract(e.value,'$.avatarId'),''),COALESCE(json_extract(e.value,'$.avatar'),'')) IN(VALUES{})", values.join(","))
    };
    let matches = if avatar_id.starts_with("avatar:") {
        assigned
    } else {
        let avatar_binds = registry
            .terms(&avatar_id)
            .into_iter()
            .map(|v| bind(Sql::Text(v)))
            .collect::<Vec<_>>()
            .join(",");
        format!("({assigned} OR json_extract(e.value,'$.avatarId') IN({avatar_binds}) OR (COALESCE(json_extract(e.value,'$.avatarId'),'') NOT LIKE 'booth:%' AND COALESCE(json_extract(e.value,'$.avatarId'),'') NOT LIKE 'product:%' AND COALESCE(json_extract(e.value,'$.avatarId'),'') NOT LIKE 'avatar:%' AND json_extract(e.value,'$.avatar') IN({avatar_binds})))")
    };
    let scope = if account_ids.is_some() {
        let account_scope = if account_placeholders.is_empty() {
            "0=1".to_owned()
        } else {
            format!("(json_extract(e.value,'$.source') IN('user','sourceTag','userTag') OR json_extract(e.value,'$.accountId') IN({}))",account_placeholders.join(","))
        };
        if include_offline {
            format!("({local_product} OR {account_scope})")
        } else {
            account_scope
        }
    } else {
        "1=1".into()
    };
    conditions.push(format!(
        "EXISTS(SELECT 1 FROM json_each(p.evidence) e WHERE {matches} AND {scope})"
    ));
    Ok(())
}

/// The list query: the filters, then the chosen order with empty values last.
fn list_sql(
    args: &api::QueryRequest,
    sort: &str,
    account_ids: Option<&Vec<String>>,
    account_placeholders: &[String],
    local_product: &str,
    conditions: &[String],
) -> String {
    let price_order = if account_ids.is_none() {
        "p.price".to_owned()
    } else {
        let scope = if account_placeholders.is_empty() {
            "0=1".to_owned()
        } else {
            format!("a.value IN({})", account_placeholders.join(","))
        };
        format!("CASE WHEN {local_product} THEN p.price WHEN json_type(p.prices)='array' THEN (SELECT CASE WHEN COUNT(*)=1 THEN MAX(json_extract(v.value,'$.price')) END FROM json_each(p.prices) v WHERE EXISTS(SELECT 1 FROM json_each(v.value,'$.accountIds') a WHERE {scope})) ELSE p.price END")
    };
    let dir =
        if args
            .direction
            .as_deref()
            .unwrap_or(if sort == "acquired" { "desc" } else { "asc" })
            == "desc"
        {
            "DESC"
        } else {
            "ASC"
        };
    let predicate = conditions.join(" AND ");
    let empty_last = |order: &str, group: &str| {
        format!("({order} IS NULL OR {order}='') ASC,{group}{order} {dir}")
    };
    let order_by = match sort {
        "title" => empty_last("p.title", ""),
        // Group original currencies; values in different currencies are not comparable.
        // Existing BOOTH projections predate priceCurrency and remain JPY.
        "price" => empty_last(&price_order, "p.currency ASC,"),
        "status" => empty_last("NULLIF(p.status,'unknown')", ""),
        _ => acquisition::order_by(
            &acquisition::sort_key_sql(account_ids.map(|_| account_placeholders), local_product),
            dir,
        ),
    };
    format!("SELECT p.id,p.downloaded FROM temp.list_rows p WHERE {predicate} ORDER BY {order_by},p.id ASC")
}

/// The requested page of products, scoped to the selected accounts; a list client gets only
/// the summaries it does not already hold, with their fingerprints.
fn page_products(
    conn: &Connection,
    args: &api::QueryRequest,
    list: &ListSnapshot,
    ids: &[String],
    account_ids: Option<&Vec<String>>,
) -> Result<(Vec<Value>, Option<BTreeMap<String, String>>)> {
    let summaries = args.include_files == Some(false);
    let list_presentation = args.presentation.as_deref() == Some("list");
    let offset = args.offset.unwrap_or(0) as usize;
    let limit = if args.all_summaries == Some(true) && summaries && list_presentation {
        ids.len()
    } else {
        args.limit.unwrap_or(100).min(10000) as usize
    };
    // A list client sends the fingerprints of summaries it already holds. Only
    // new or changed summaries cross IPC; fingerprints include the account scope.
    let known = if summaries {
        args.known_summaries.as_ref()
    } else {
        None
    };
    let scope = {
        let mut hasher = DefaultHasher::new();
        (
            serde_json::to_string(&args.account_ids)?,
            serde_json::to_string(&args.presentation)?,
        )
            .hash(&mut hasher);
        hasher.finish()
    };
    let mut fingerprints = BTreeMap::new();
    // The account scope as `apply_scope` reads it.
    let scope_accounts: Option<Vec<Value>> =
        account_ids.map(|ids| ids.iter().map(|id| json!(id)).collect());
    let mut products = vec![];
    let mut product_stmt = conn.prepare_cached(if summaries {
        "SELECT summary,unavailable FROM temp.list_rows WHERE id=?1"
    } else {
        "SELECT p.body,l.unavailable FROM products p JOIN temp.list_rows l ON l.id=p.id WHERE p.id=?1"
    })?;
    for id in ids.iter().skip(offset).take(limit) {
        if let Some(known) = known {
            let mut hasher = DefaultHasher::new();
            (list.summaries.get(id), scope).hash(&mut hasher);
            let fingerprint = format!("{:016x}", hasher.finish());
            if known.get(id) == Some(&fingerprint) {
                continue;
            }
            fingerprints.insert(id.clone(), fingerprint);
        }
        let (body, unavailable): (String, bool) =
            product_stmt.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut product = serde_json::from_str::<Value>(&body)?;
        // List summaries carry the derived fact; stored product bodies stay unchanged.
        if summaries {
            product["unavailable"] = json!(unavailable);
        }
        acquisition::apply_scope(&mut product, scope_accounts.as_ref());
        // Compact transport only after native filtering and scoped financial
        // calculations. Keep the facts used by cached list/editor consumers.
        if summaries && list_presentation {
            compact_for_list(&mut product);
        }
        products.push(product);
    }
    Ok((products, known.map(|_| fingerprints)))
}

/// List summaries drop what only the inspector reads.
pub(super) fn compact_for_list(product: &mut Value) {
    if let Some(fields) = product.as_object_mut() {
        for field in [
            "provenance",
            "categoryInference",
            "sources",
            "sourceShop",
            "sourceOriginUrl",
            "publicPageAvailability",
            "store",
            "sourceKind",
        ] {
            fields.remove(field);
        }
        for (field, keep) in [
            ("tagSuggestions", &["label"][..]),
            (
                "ownedAvatarEvidence",
                &["avatar", "source", "accountId"][..],
            ),
        ] {
            if let Some(rows) = fields.get_mut(field).and_then(Value::as_array_mut) {
                for row in rows {
                    if let Some(row) = row.as_object_mut() {
                        row.retain(|key, _| keep.contains(&key.as_str()));
                    }
                }
            }
        }
    }
}

/// The saved list preferences: unavailable products, terms-only ones, and Unclassified ones
/// in All, are hidden unless turned off.
struct ListPreferences {
    unavailable: bool,
    terms_only: bool,
    unclassified: bool,
}

fn list_preferences(conn: &Connection) -> Result<ListPreferences> {
    let preferences = raw(conn, "preferences", "app")?.unwrap_or(json!({}));
    Ok(ListPreferences {
        unavailable: preferences["hideUnavailable"] != false,
        terms_only: preferences["hideTermsOnly"] != false,
        unclassified: preferences["hideUnclassified"] != false,
    })
}

/// Visible account totals are independent of query filters and visited pages.
pub(super) fn account_counts(conn: &Connection, cache: &mut ListSnapshot) -> Result<Value> {
    snapshot(conn, cache)?;
    let mut accounts = BTreeMap::new();
    let mut known = conn.prepare("SELECT id FROM entities WHERE kind='account'")?;
    for id in known.query_map([], |row| row.get::<_, String>(0))? {
        accounts.insert(id?, 0);
    }
    let preferences = list_preferences(conn)?;
    let mut visible = "p.hidden=0".to_owned();
    if preferences.unavailable {
        visible.push_str(" AND p.unavailable=0");
    }
    if preferences.terms_only {
        visible.push_str(" AND p.terms_only=0");
    }
    let mut statement = conn.prepare(&format!("SELECT a.value,COUNT(DISTINCT p.id) FROM temp.list_rows p,json_each(p.accounts) a JOIN entities e ON e.kind='account' AND e.id=a.value WHERE {visible} GROUP BY a.value"))?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (account, count) = row?;
        accounts.insert(account, count);
    }
    let unassigned: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM temp.list_rows p WHERE {visible} AND COALESCE(json_extract(p.summary,'$.custom'),0)<>1 AND NOT EXISTS(SELECT 1 FROM json_each(p.accounts) a JOIN entities e ON e.kind='account' AND e.id=a.value)"), [], |row| row.get(0))?;
    Ok(serde_json::to_value(api::AccountCounts {
        accounts,
        unassigned,
    })?)
}

pub(super) fn counts(
    conn: &Connection,
    cache: &mut ListSnapshot,
) -> Result<(i64, api::ListCounts)> {
    let preferences = list_preferences(conn)?;
    let list = snapshot(conn, cache)?;
    Ok((
        list.total,
        list.counts[hidden_set(preferences.unavailable, preferences.terms_only)]
            [usize::from(preferences.unclassified)]
        .clone(),
    ))
}
