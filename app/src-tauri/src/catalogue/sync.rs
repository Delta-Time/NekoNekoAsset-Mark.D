//! Portable metadata CRDT. SQLite owns the durable document and projection transaction.
use anyhow::{bail, ensure, Context, Result};
use automerge::{transaction::Transactable, ActorId, AutoCommit, Change, ReadDoc, ROOT};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const DELETED: &str = "@deleted";
const MEMBER: &str = "@member:";
const KINDS: &[&str] = &[
    "product",
    "overlay",
    "acquisition",
    "entitlement",
    "remote_file",
    "collection",
    "saved_search",
    "custom_category",
    "source_coverage",
];

pub fn new_doc(actor: &str) -> Result<AutoCommit> {
    ensure!(!actor.is_empty(), "sync_actor_required");
    let mut doc = AutoCommit::new();
    doc.set_actor(ActorId::from(Sha256::digest(actor.as_bytes()).to_vec()));
    Ok(doc)
}

pub fn load_doc(bytes: &[u8], actor: &str) -> Result<AutoCommit> {
    let mut doc = load_trusted_doc(bytes, actor)?;
    validate_doc(&mut doc)?;
    Ok(doc)
}

// Only checksummed SQLite checkpoints/deltas previously validated by this version.
pub(super) fn load_trusted_doc(bytes: &[u8], actor: &str) -> Result<AutoCommit> {
    let mut doc = if bytes.is_empty() {
        new_doc(actor)?
    } else {
        AutoCommit::load(bytes).map_err(|_| anyhow::anyhow!("sync_document_invalid"))?
    };
    ensure!(!actor.is_empty(), "sync_actor_required");
    doc.set_actor(ActorId::from(Sha256::digest(actor.as_bytes()).to_vec()));
    ensure!(
        doc.get_missing_deps(&[]).is_empty(),
        "sync_document_dependencies_missing"
    );
    Ok(doc)
}

fn check_identity(kind: &str, id: &str) -> Result<()> {
    ensure!(KINDS.contains(&kind), "sync_kind_not_portable");
    ensure!(!id.is_empty() && id.len() <= 4096, "sync_identity_invalid");
    Ok(())
}

fn key(kind: &str, id: &str, field: &str) -> String {
    json!([kind, id, field]).to_string()
}

fn parse_key(key: &str) -> Result<(String, String, String)> {
    let parts: Vec<String> = serde_json::from_str(key).context("sync_key_invalid")?;
    ensure!(parts.len() == 3, "sync_key_invalid");
    check_identity(&parts[0], &parts[1])?;
    Ok((parts[0].clone(), parts[1].clone(), parts[2].clone()))
}

fn check_value(value: &Value, depth: usize) -> Result<()> {
    ensure!(depth <= 32, "sync_value_too_deep");
    match value {
        Value::Object(map) => {
            for (field, value) in map {
                let normalized: String = field
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect();
                ensure!(
                    ![
                        "cookie",
                        "cookies",
                        "token",
                        "accesstoken",
                        "refreshtoken",
                        "csrftoken",
                        "apikey",
                        "password",
                        "credentials",
                        "secret",
                        "secrets",
                        "session",
                        "sessionid",
                        "authorization",
                        "headers",
                        "rawbody",
                        "httpbody",
                        "rawresponse",
                        "rawhtml",
                        "privatebody",
                        "downloadurl",
                        "signeddownloadurl",
                        "signedurl",
                        "localpath",
                        "absolutepath",
                        "rootpath",
                        "profilepath",
                        "path",
                        "extractedpath",
                        "binary",
                        "imagedata",
                        "thumbnaildata"
                    ]
                    .contains(&normalized.as_str()),
                    "sync_private_field"
                );
                if normalized.ends_with("path") {
                    if let Some(path) = value.as_str() {
                        ensure!(
                            !path.starts_with('/')
                                && !path.starts_with('\\')
                                && !path.contains(':')
                                && !path.split(['/', '\\']).any(|p| p == ".."),
                            "sync_device_path"
                        );
                    }
                }
                check_value(value, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                check_value(value, depth + 1)?;
            }
        }
        Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            ensure!(
                !lower.starts_with("data:")
                    && !lower.starts_with("blob:")
                    && !lower.starts_with("file:"),
                "sync_binary_or_local_url"
            );
            if lower.starts_with("https://") || lower.starts_with("http://") {
                ensure!(
                    ![
                        "x-amz-",
                        "signature=",
                        "token=",
                        "expires=",
                        "policy=",
                        "key-pair-id="
                    ]
                    .iter()
                    .any(|part| lower.contains(part)),
                    "sync_signed_url"
                );
            }
        }
        _ => (),
    }
    Ok(())
}

// A missing field is a register value, not an Automerge delete: concurrent cleared
// fields must remain visible in get_all alongside the other device's value.
fn present(value: &Value) -> Value {
    json!({"present":true,"value":value})
}
fn absent() -> Value {
    json!({"present":false})
}

fn check_entity_value(kind: &str, value: &Map<String, Value>) -> Result<()> {
    let mut portable = value.clone();
    if kind == "overlay" {
        if let Some(url) = portable.remove("sourceUrl") {
            super::validate_source_url(url.as_str().context("sync_source_url_invalid")?)?;
        }
        if let Some(url) = portable.remove("sourceThumbnailUrl") {
            super::validate_source_thumbnail_url(url.as_str().context("sync_source_url_invalid")?)?;
        }
    }
    check_value(&Value::Object(portable), 0)
}

fn validate_register(kind: &str, field: &str, value: &Value) -> Result<()> {
    if field == DELETED {
        ensure!(value.is_boolean(), "sync_tombstone_invalid");
    } else if let Some(member) = field.strip_prefix(MEMBER) {
        ensure!(kind == "collection", "sync_member_invalid");
        let pair: Vec<String> = serde_json::from_str(member).context("sync_member_invalid")?;
        ensure!(
            pair.len() == 2 && pair[0] == "productIds" && !pair[1].is_empty() && value.is_boolean(),
            "sync_member_invalid"
        );
    } else {
        ensure!(
            !field.is_empty() && !field.starts_with('@'),
            "sync_field_invalid"
        );
        ensure!(
            value.get("present").and_then(Value::as_bool).is_some(),
            "sync_register_invalid"
        );
        ensure!(
            value.as_object().map(|v| v.len())
                == Some(if value["present"] == true { 2 } else { 1 }),
            "sync_register_invalid"
        );
        if value["present"] == true {
            ensure!(value.get("value").is_some(), "sync_register_invalid");
            if kind == "overlay" && field == "sourceUrl" {
                super::validate_source_url(
                    value["value"].as_str().context("sync_source_url_invalid")?,
                )?;
            } else if kind == "overlay" && field == "sourceThumbnailUrl" {
                super::validate_source_thumbnail_url(
                    value["value"].as_str().context("sync_source_url_invalid")?,
                )?;
            } else {
                check_value(&json!({field:value["value"]}), 0)?;
            }
            if kind == "overlay" && field == "thumbnailPath" {
                let path = value["value"].as_str().context("sync_device_path")?;
                ensure!(
                    !path.is_empty()
                        && !path.contains([':', '\0'])
                        && !path.starts_with(['/', '\\'])
                        && !path
                            .split(['/', '\\'])
                            .any(|part| part.is_empty() || part == "." || part == ".."),
                    "sync_device_path"
                );
            }
            if kind == "collection" && field == "productIds" {
                ensure!(value["value"] == json!([]), "sync_member_invalid");
            }
        }
    }
    Ok(())
}

fn register_values(doc: &AutoCommit, key: &str) -> Result<Vec<(String, Value)>> {
    doc.get_all(ROOT, key)?
        .into_iter()
        .map(|(value, op)| {
            let text = value.to_str().context("sync_scalar_invalid")?;
            Ok((
                op.to_string(),
                serde_json::from_str(text).context("sync_scalar_invalid")?,
            ))
        })
        .collect()
}

fn effective(doc: &AutoCommit, key: &str) -> Result<Option<Value>> {
    doc.get(ROOT, key)?
        .map(|(value, _)| {
            serde_json::from_str(value.to_str().context("sync_scalar_invalid")?)
                .context("sync_scalar_invalid")
        })
        .transpose()
}

/// Applies only fields changed between before/after. Parent must provide the current
/// portable source entity, never the effective source+overlay+local-files projection.
pub fn write_entity(
    doc: &mut AutoCommit,
    kind: &str,
    id: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> Result<()> {
    check_identity(kind, id)?;
    let empty = Map::new();
    // Reusing an explicitly deleted identity replaces its old fields/members.
    // Historical values still exist in Automerge, but must not leak into a newly
    // created entity whose caller legitimately has no live `before` record.
    let retained = if before.is_none()
        && after.is_some()
        && doc.get(ROOT, key(kind, id, DELETED))?.is_some()
    {
        project(doc, true)?
            .into_iter()
            .find(|(k, i, _)| k == kind && i == id)
            .map(|(_, _, v)| v)
    } else {
        None
    };
    let recreating = retained.is_some();
    let previous = before
        .or(retained.as_ref())
        .map(|v| v.as_object().context("sync_entity_invalid"))
        .transpose()?
        .unwrap_or(&empty);
    let next = after
        .map(|v| v.as_object().context("sync_entity_invalid"))
        .transpose()?;
    if let Some(next) = next {
        check_entity_value(kind, next)?;
        ensure!(
            next.keys()
                .all(|field| !field.is_empty() && !field.starts_with('@')),
            "sync_field_invalid"
        );
        if let Some(value) = next.get("id") {
            ensure!(value.as_str() == Some(id), "sync_identity_mismatch");
        }
        if kind == "collection" {
            members(next.get("productIds"))?;
        }
    }
    let mut edits: Vec<(String, Value)> = Vec::new();
    match next {
        None => edits.push((key(kind, id, DELETED), json!(true))),
        Some(next) => {
            // An ordinary edit cannot resurrect a concurrent deletion.
            if before.is_none() || effective(doc, &key(kind, id, DELETED))?.is_none() {
                edits.push((key(kind, id, DELETED), json!(false)));
            }
            for field in previous.keys().chain(next.keys()).collect::<BTreeSet<_>>() {
                if !recreating && previous.get(field) == next.get(field) {
                    continue;
                }
                if kind == "collection" && field == "productIds" {
                    let old = members(previous.get(field))?;
                    let new = members(next.get(field))?;
                    for member in old.union(&new) {
                        if recreating || old.contains(member) != new.contains(member) {
                            edits.push((
                                key(kind, id, &format!("{MEMBER}{}", json!([field, member]))),
                                json!(new.contains(member)),
                            ));
                        }
                    }
                    edits.push((
                        key(kind, id, field),
                        if next.contains_key(field) {
                            present(&json!([]))
                        } else {
                            absent()
                        },
                    ));
                } else {
                    edits.push((
                        key(kind, id, field),
                        next.get(field).map(present).unwrap_or_else(absent),
                    ));
                }
            }
        }
    }
    // All fallible input checks precede mutation; SQLite supplies transactionality.
    for (key, value) in edits {
        // Recreated rows emit even unchanged retained fields, so the recreating
        // change holds the complete row. Automerge otherwise optimizes an equal put away.
        if recreating {
            doc.delete(ROOT, key.as_str())?;
        }
        doc.put(ROOT, key, value.to_string())?;
    }
    Ok(())
}

/// The SQL foreground rejects invalid portable values without loading history.
pub(super) fn validate_entity(kind: &str, id: &str, after: Option<&Value>) -> Result<()> {
    check_identity(kind, id)?;
    if let Some(after) = after {
        let object = after.as_object().context("sync_entity_invalid")?;
        check_entity_value(kind, object)?;
        ensure!(
            object
                .keys()
                .all(|field| !field.is_empty() && !field.starts_with('@')),
            "sync_field_invalid"
        );
        ensure!(after["id"].as_str() == Some(id), "sync_identity_mismatch");
        if kind == "collection" {
            members(object.get("productIds"))?;
        }
    }
    Ok(())
}

fn members(value: Option<&Value>) -> Result<BTreeSet<String>> {
    match value {
        None => Ok(BTreeSet::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .context("sync_member_invalid")
            })
            .collect(),
        _ => bail!("sync_member_invalid"),
    }
}

/// Deleted entities are absent from this complete projection; parent removes their
/// portable SQLite rows in the same transaction that stores the document bytes.
pub fn entities(doc: &AutoCommit) -> Result<Vec<(String, String, Value)>> {
    project(doc, false)
}

fn project(doc: &AutoCommit, include_deleted: bool) -> Result<Vec<(String, String, Value)>> {
    let mut records: BTreeMap<(String, String), Map<String, Value>> = BTreeMap::new();
    let mut deleted = BTreeSet::new();
    let mut membership: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for field_key in doc.keys(ROOT) {
        let (kind, id, field) = parse_key(&field_key)?;
        let identity = (kind.clone(), id.clone());
        let values = register_values(doc, &field_key)?;
        for (_, value) in &values {
            validate_register(&kind, &field, value)?;
        }
        if field == DELETED {
            if values.iter().any(|(_, v)| *v == true) {
                deleted.insert(identity.clone());
            }
            records.entry(identity).or_default();
        } else if let Some(member) = field.strip_prefix(MEMBER) {
            let pair: Vec<String> = serde_json::from_str(member)?;
            // Add wins against a concurrent removal; a later observed removal wins.
            if values.iter().any(|(_, v)| *v == true) {
                membership
                    .entry(identity)
                    .or_default()
                    .insert(pair[1].clone());
            }
        } else if let Some(value) = effective(doc, &field_key)? {
            if value["present"] == true {
                records
                    .entry(identity)
                    .or_default()
                    .insert(field, value["value"].clone());
            }
        }
    }
    for (identity, ids) in membership {
        records
            .entry(identity)
            .or_default()
            .insert("productIds".to_owned(), json!(ids));
    }
    Ok(records
        .into_iter()
        .filter(|(identity, _)| include_deleted || !deleted.contains(identity))
        .map(|((kind, id), mut value)| {
            value.insert("id".into(), json!(id));
            (kind, id, Value::Object(value))
        })
        .collect())
}

// Validate EVERY historical operation, not only the current winners: otherwise an
// overwritten secret could be smuggled into an apparently safe portable document.
fn validate_change(change: &Change) -> Result<()> {
    let expanded = serde_json::to_value(change.decode())?;
    ensure!(
        expanded["message"].is_null(),
        "sync_change_message_forbidden"
    );
    ensure!(
        expanded.get("extra_bytes").is_none(),
        "sync_change_extra_bytes_forbidden"
    );
    for op in expanded["ops"].as_array().context("sync_change_invalid")? {
        validate_operation(op)?;
    }
    Ok(())
}

fn validate_operation(op: &Value) -> Result<(String, String, String)> {
    ensure!(op["obj"] == "_root", "sync_operation_invalid");
    let (kind, id, field) = parse_key(op["key"].as_str().context("sync_key_invalid")?)?;
    let object = op.as_object().context("sync_operation_invalid")?;
    ensure!(op["pred"].is_array(), "sync_operation_invalid");
    if op["action"] == "set" {
        ensure!(
            object.len() == 5
                && object
                    .keys()
                    .all(|key| ["action", "obj", "key", "pred", "value"].contains(&key.as_str())),
            "sync_operation_invalid"
        );
        let register: Value =
            serde_json::from_str(op["value"].as_str().context("sync_scalar_invalid")?)?;
        validate_register(&kind, &field, &register)?;
    } else if op["action"] == "del" {
        // Automerge 0.7.4 OpsFound::resolve_action emits a native delete of
        // losing predecessors when put selects the existing scalar winner.
        // A delete carries no value/type/insert payload; historical sets are
        // still individually validated, including values later deleted here.
        ensure!(
            object.len() == 4
                && object
                    .keys()
                    .all(|key| ["action", "obj", "key", "pred"].contains(&key.as_str())),
            "sync_operation_invalid"
        );
        ensure!(
            !op["pred"].as_array().unwrap().is_empty(),
            "sync_operation_invalid"
        );
        let probe = if field == DELETED || field.starts_with(MEMBER) {
            json!(false)
        } else if kind == "collection" && field == "productIds" {
            present(&json!([]))
        } else if kind == "overlay" && field == "thumbnailPath" {
            // A native delete has no value. Validate its field policy using a
            // valid relative-path probe; historical sets validate actual paths.
            present(&json!("media/validation.png"))
        } else if kind == "overlay" && matches!(field.as_str(), "sourceUrl" | "sourceThumbnailUrl")
        {
            // Deletions have no URL payload; historical sets retain full checks.
            present(&json!(""))
        } else {
            present(&Value::Null)
        };
        validate_register(&kind, &field, &probe)?;
    } else {
        bail!("sync_operation_invalid");
    }
    Ok((kind, id, field))
}

fn validate_doc(doc: &mut AutoCommit) -> Result<()> {
    for change in doc.get_changes(&[]) {
        validate_change(&change)?;
    }
    entities(doc)?;
    Ok(())
}

fn validated_snapshot(bytes: &[u8]) -> Result<AutoCommit> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 256 * 1024 * 1024,
        "sync_snapshot_size_invalid"
    );
    let mut snapshot =
        AutoCommit::load(bytes).map_err(|_| anyhow::anyhow!("sync_snapshot_invalid"))?;
    ensure!(
        snapshot.get_missing_deps(&[]).is_empty(),
        "sync_snapshot_dependencies_missing"
    );
    validate_doc(&mut snapshot)?;
    Ok(snapshot)
}

/// Preflight an untrusted portable snapshot without modifying the live document.
/// The manifest owner additionally checks its version and external SHA256 envelope.
pub fn validate_snapshot(bytes: &[u8]) -> Result<()> {
    validated_snapshot(bytes)?;
    Ok(())
}

fn semantic_register(doc: &AutoCommit, field_key: &str) -> Result<Option<Value>> {
    let (_, _, field) = parse_key(field_key)?;
    if field == DELETED || field.starts_with(MEMBER) {
        let values = register_values(doc, field_key)?;
        Ok(if values.is_empty() {
            None
        } else {
            Some(json!(values.iter().any(|(_, value)| *value == true)))
        })
    } else {
        effective(doc, field_key)
    }
}

fn distinct_registers(values: &[(String, Value)]) -> BTreeMap<String, Value> {
    values
        .iter()
        .map(|(_, value)| (value.to_string(), value.clone()))
        .collect()
}

/// Restore portable semantics onto existing causal history. The caller's actor
/// remains unchanged. A register with concurrent values in the snapshot keeps
/// its effective value, which for a field is Automerge's winner.
/// Work happens in a candidate; validation/postcondition failures leave `doc` intact.
pub fn restore_snapshot(
    doc: &mut AutoCommit,
    bytes: &[u8],
    desired: &[(String, String, Value)],
) -> Result<()> {
    let mut snapshot = validated_snapshot(bytes)?;
    let mut wanted = BTreeMap::new();
    for (kind, id, value) in desired {
        check_identity(kind, id)?;
        let mut value = value.as_object().cloned().context("sync_entity_invalid")?;
        ensure!(
            value.get("id").and_then(Value::as_str) == Some(id.as_str()),
            "sync_identity_mismatch"
        );
        if kind == "collection" && value.contains_key("productIds") {
            value.insert(
                "productIds".into(),
                json!(members(value.get("productIds"))?),
            );
        }
        check_entity_value(kind, &value)?;
        ensure!(
            wanted
                .insert((kind.clone(), id.clone()), Value::Object(value))
                .is_none(),
            "sync_snapshot_duplicate_entity"
        );
    }
    let mut saved = BTreeMap::new();
    let mut saved_identities = BTreeSet::new();
    for field_key in snapshot.keys(ROOT) {
        let (kind, id, _) = parse_key(&field_key)?;
        saved_identities.insert((kind, id));
        let winner =
            semantic_register(&snapshot, &field_key)?.context("sync_snapshot_register_missing")?;
        let choices = distinct_registers(&register_values(&snapshot, &field_key)?);
        saved.insert(field_key, (winner, choices));
    }
    let mut candidate = doc.clone();
    candidate.merge(&mut snapshot)?;
    let mut writes = 0;
    // Restore all retained registers, including fields of deleted entities, which
    // live-only JSON records cannot represent.
    for (field_key, (winner, choices)) in &saved {
        if semantic_register(&candidate, field_key)?.as_ref() != Some(winner)
            || distinct_registers(&register_values(&candidate, field_key)?) != *choices
        {
            candidate.put(ROOT, field_key.as_str(), winner.to_string())?;
            writes += 1;
            if writes % 128 == 0 {
                candidate.commit();
            }
        }
    }
    let candidate_keys: Vec<String> = candidate.keys(ROOT).collect();
    for field_key in candidate_keys {
        let (kind, id, field) = parse_key(&field_key)?;
        if saved_identities.contains(&(kind, id)) && !saved.contains_key(&field_key) {
            let value = if field == DELETED || field.starts_with(MEMBER) {
                json!(false)
            } else {
                absent()
            };
            candidate.put(ROOT, field_key, value.to_string())?;
            writes += 1;
            if writes % 128 == 0 {
                candidate.commit();
            }
        }
    }
    let current: BTreeMap<(String, String), Value> = entities(&candidate)?
        .into_iter()
        .map(|(kind, id, value)| ((kind, id), value))
        .collect();
    for ((kind, id), value) in &current {
        if !wanted.contains_key(&(kind.clone(), id.clone())) {
            write_entity(&mut candidate, kind, id, Some(value), None)?;
        }
    }
    for ((kind, id), value) in &wanted {
        write_entity(
            &mut candidate,
            kind,
            id,
            current.get(&(kind.clone(), id.clone())),
            Some(value),
        )?;
        writes += 1;
        if writes % 128 == 0 {
            candidate.commit();
        }
    }
    let actual: BTreeMap<(String, String), Value> = entities(&candidate)?
        .into_iter()
        .map(|(kind, id, value)| ((kind, id), value))
        .collect();
    ensure!(actual == wanted, "sync_snapshot_projection_mismatch");
    ensure!(
        candidate.get_missing_deps(&[]).is_empty(),
        "sync_snapshot_dependencies_missing"
    );
    *doc = candidate;
    Ok(())
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
