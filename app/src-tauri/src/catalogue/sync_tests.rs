
use super::*;

#[test]
fn fetched_source_thumbnail_register_rejects_unsafe_history_and_retains_empty() -> Result<()> {
    let safe = "https://booth.pximg.net/synthetic-fetched.png";
    for value in [safe, ""] {
        validate_register("overlay", "sourceThumbnailUrl", &present(&json!(value)))?;
    }
    for value in [
        "http://booth.pximg.net/a",
        "https://example.org/a",
        "https://booth.pximg.net/a?token=synthetic",
        "https://booth.pximg.net:443/a",
        "https://u:p@booth.pximg.net/a",
    ] {
        assert!(
            validate_register("overlay", "sourceThumbnailUrl", &present(&json!(value))).is_err()
        );
        let mut history = new_doc("synthetic-thumbnail-history")?;
        history.put(
            ROOT,
            key("overlay", "p", "sourceThumbnailUrl"),
            present(&json!(value)).to_string(),
        )?;
        history.commit();
        history.put(
            ROOT,
            key("overlay", "p", "sourceThumbnailUrl"),
            present(&json!(safe)).to_string(),
        )?;
        assert!(validate_snapshot(&history.save()).is_err());
    }
    let mut doc = new_doc("synthetic-thumbnail-device")?;
    let value = json!({"id":"p","sourceThumbnailUrl":""});
    write_entity(&mut doc, "overlay", "p", None, Some(&value))?;
    doc = load_doc(&doc.save(), "synthetic-thumbnail-device")?;
    assert_eq!(entity(&doc, "overlay", "p").unwrap(), value);
    doc.delete(ROOT, key("overlay", "p", "sourceThumbnailUrl"))?;
    validate_snapshot(&doc.save())?;
    Ok(())
}

#[test]
fn overlay_source_urls_roundtrip_and_unsafe_historical_values_are_rejected() -> Result<()> {
    let safe = "https://example.org/custom-assets/42?variant=blue#files";
    for url in [
        "",
        "http://example.org/asset",
        safe,
        "https://example.org/assets/token=public",
    ] {
        validate_register("overlay", "sourceUrl", &present(&json!(url)))?;
    }
    for url in [
        "javascript:alert(1)",
        "file:///synthetic.asset",
        "ftp://example.org/asset",
        "https://synthetic-user:synthetic-password@example.org/asset",
        "/relative/asset",
        "https://example.org/asset?token=synthetic",
        "https://example.org/asset?signature=synthetic",
        "https://example.org/asset?X-Amz-Signature=synthetic",
    ] {
        assert!(validate_register("overlay", "sourceUrl", &present(&json!(url))).is_err());
        let mut history = new_doc("unsafe-url-history")?;
        history.put(
            ROOT,
            key("overlay", "p", "sourceUrl"),
            present(&json!(url)).to_string(),
        )?;
        history.commit();
        history.put(
            ROOT,
            key("overlay", "p", "sourceUrl"),
            present(&json!(safe)).to_string(),
        )?;
        assert!(validate_snapshot(&history.save()).is_err());
    }
    let mut doc = new_doc("source-url-device")?;
    let before = json!({"id":"p","sourceUrl":safe});
    write_entity(&mut doc, "overlay", "p", None, Some(&before))?;
    doc = load_doc(&doc.save(), "source-url-device")?;
    assert_eq!(entity(&doc, "overlay", "p").unwrap(), before);
    let cleared = json!({"id":"p","sourceUrl":""});
    write_entity(&mut doc, "overlay", "p", Some(&before), Some(&cleared))?;
    doc = load_doc(&doc.save(), "source-url-device")?;
    assert_eq!(entity(&doc, "overlay", "p").unwrap(), cleared);
    doc.delete(ROOT, key("overlay", "p", "sourceUrl"))?;
    validate_snapshot(&doc.save())?;
    Ok(())
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/catalogue-synthetic")
            .join(format!("nna-sync-synthetic-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn entity(doc: &AutoCommit, kind: &str, id: &str) -> Option<Value> {
    entities(doc)
        .unwrap()
        .into_iter()
        .find(|(k, i, _)| k == kind && i == id)
        .map(|(_, _, v)| v)
}

#[test]
fn two_devices_offline_fields_conflicts_restart_and_resolution() -> Result<()> {
    let folder = Scratch::new();
    let mut a = new_doc("synthetic-device-a")?;
    let baseline =
        json!({"id":"p1","name":"Original","notes":"Initial","rejectedTags":["rejected"]});
    write_entity(&mut a, "overlay", "p1", None, Some(&baseline))?;
    exchange(&mut a, &folder.0)?;
    let mut b = new_doc("synthetic-device-b")?;
    exchange(&mut b, &folder.0)?;
    let av = json!({"id":"p1","name":"A name","notes":"A note","rejectedTags":["rejected"]});
    let bv =
        json!({"id":"p1","name":"B name","notes":"Initial","rejectedTags":["rejected","another"]});
    write_entity(&mut a, "overlay", "p1", Some(&baseline), Some(&av))?;
    write_entity(&mut b, "overlay", "p1", Some(&baseline), Some(&bv))?;
    a = load_doc(&a.save(), "synthetic-device-a")?;
    b = load_doc(&b.save(), "synthetic-device-b")?;
    exchange(&mut a, &folder.0)?;
    exchange(&mut b, &folder.0)?;
    exchange(&mut a, &folder.0)?;
    assert_eq!(entities(&a)?, entities(&b)?);
    let merged = entity(&a, "overlay", "p1").unwrap();
    assert_eq!(merged["notes"], "A note");
    assert_eq!(merged["rejectedTags"], bv["rejectedTags"]);
    let conflicts = conflicts(&a)?;
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0]["field"], "name");
    let op = conflicts[0]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["value"]["value"] == "A name")
        .unwrap()["opId"]
        .clone();
    resolve(
        &mut a,
        &json!({"kind":"overlay","id":"p1","field":"name","opId":op}),
    )?;
    exchange(&mut a, &folder.0)?;
    exchange(&mut b, &folder.0)?;
    assert_eq!(entity(&b, "overlay", "p1").unwrap()["name"], "A name");
    assert!(super::conflicts(&b)?.is_empty());
    let repeat = exchange(&mut b, &folder.0)?;
    assert_eq!(repeat["imported"], 0);
    assert_eq!(repeat["exported"], 0);
    Ok(())
}

#[test]
fn collections_independent_additions_and_delete_vs_edit_survive_restart() -> Result<()> {
    let mut a = new_doc("a")?;
    let initial = json!({"id":"c","name":"Collection","productIds":[]});
    write_entity(&mut a, "collection", "c", None, Some(&initial))?;
    let mut b = load_doc(&a.save(), "b")?;
    let av = json!({"id":"c","name":"Collection","productIds":["one"]});
    let bv = json!({"id":"c","name":"Collection","productIds":["two"]});
    write_entity(&mut a, "collection", "c", Some(&initial), Some(&av))?;
    write_entity(&mut b, "collection", "c", Some(&initial), Some(&bv))?;
    a.merge(&mut b)?;
    b.merge(&mut a)?;
    assert_eq!(
        entity(&a, "collection", "c").unwrap()["productIds"],
        json!(["one", "two"])
    );
    let before = entity(&a, "collection", "c").unwrap();
    write_entity(&mut a, "collection", "c", Some(&before), None)?;
    let mut edited = before.clone();
    edited["name"] = json!("Offline edit");
    write_entity(&mut b, "collection", "c", Some(&before), Some(&edited))?;
    a.merge(&mut b)?;
    a = load_doc(&a.save(), "a")?;
    assert!(entity(&a, "collection", "c").is_none());
    resolve(
        &mut a,
        &json!({"kind":"collection","id":"c","field":DELETED,"value":false}),
    )?;
    assert_eq!(
        entity(&a, "collection", "c").unwrap()["name"],
        "Offline edit"
    );
    let restored = entity(&a, "collection", "c").unwrap();
    write_entity(&mut a, "collection", "c", Some(&restored), None)?;
    write_entity(
        &mut a,
        "collection",
        "c",
        None,
        Some(&json!({"id":"c","name":"Fresh","productIds":[]})),
    )?;
    assert_eq!(
        entity(&a, "collection", "c").unwrap()["productIds"],
        json!([])
    );
    Ok(())
}

#[test]
fn out_of_order_partial_checksum_and_private_history() -> Result<()> {
    let folder = Scratch::new();
    let mut a = new_doc("a")?;
    let first = json!({"id":"p","name":"First"});
    write_entity(&mut a, "product", "p", None, Some(&first))?;
    a.commit();
    write_entity(
        &mut a,
        "product",
        "p",
        Some(&first),
        Some(&json!({"id":"p","name":"Second"})),
    )?;
    a.commit();
    let changes = a.get_changes(&[]);
    publish(&folder.0, changes[1].raw_bytes())?;
    fs::write(folder.0.join("interrupted.partial"), b"unfinished")?;
    let mut b = new_doc("b")?;
    assert_eq!(exchange(&mut b, &folder.0)?["pendingDependencies"], 1);
    b = load_doc(&b.save(), "b")?;
    publish(&folder.0, changes[0].raw_bytes())?;
    exchange(&mut b, &folder.0)?;
    assert_eq!(entity(&b, "product", "p").unwrap()["name"], "Second");
    let before = b.save();
    fs::write(
        folder.0.join(format!("{}.amchange", "0".repeat(64))),
        b"bad",
    )?;
    assert!(exchange(&mut b, &folder.0).is_err());
    assert_eq!(b.save(), before);
    assert!(write_entity(
        &mut b,
        "account",
        "secret",
        None,
        Some(&json!({"id":"secret"}))
    )
    .is_err());
    assert!(write_entity(
        &mut b,
        "product",
        "p",
        None,
        Some(&json!({"id":"p","cookies":"private"}))
    )
    .is_err());
    let mut poison = new_doc("malformed")?;
    poison.put(
        ROOT,
        key("product", "p", "cookies"),
        present(&json!("private")).to_string(),
    )?;
    poison.commit();
    poison.put(ROOT, key("product", "p", "cookies"), absent().to_string())?;
    assert!(load_doc(&poison.save(), "b").is_err());
    Ok(())
}

#[test]
fn concurrent_field_clear_is_retained_and_can_be_selected() -> Result<()> {
    let mut a = new_doc("a")?;
    let first = json!({"id":"p","notes":"Before"});
    write_entity(&mut a, "overlay", "p", None, Some(&first))?;
    let mut b = load_doc(&a.save(), "b")?;
    write_entity(
        &mut a,
        "overlay",
        "p",
        Some(&first),
        Some(&json!({"id":"p"})),
    )?;
    write_entity(
        &mut b,
        "overlay",
        "p",
        Some(&first),
        Some(&json!({"id":"p","notes":null})),
    )?;
    a.merge(&mut b)?;
    let conflicts = conflicts(&a)?;
    assert_eq!(conflicts.len(), 1);
    let missing = conflicts[0]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["value"]["present"] == false)
        .unwrap()["opId"]
        .clone();
    resolve(
        &mut a,
        &json!({"kind":"overlay","id":"p","field":"notes","opId":missing}),
    )?;
    assert!(entity(&a, "overlay", "p").unwrap().get("notes").is_none());
    Ok(())
}

#[test]
fn cached_conflicts_refresh_only_previous_keys_and_local_write_resolves() -> Result<()> {
    let mut a = new_doc("a")?;
    let first = json!({"id":"p","name":"Before","notes":"Before"});
    write_entity(&mut a, "overlay", "p", None, Some(&first))?;
    let mut b = load_doc(&a.save(), "b")?;
    write_entity(
        &mut a,
        "overlay",
        "p",
        Some(&first),
        Some(&json!({"id":"p","name":"A","notes":"A"})),
    )?;
    write_entity(
        &mut b,
        "overlay",
        "p",
        Some(&first),
        Some(&json!({"id":"p","name":"B","notes":"B"})),
    )?;
    a.merge(&mut b)?;
    let previous = conflicts(&a)?;
    assert_eq!(previous.len(), 2);
    assert_eq!(remaining_conflicts(&a, &previous)?, previous);
    let notes = previous
        .iter()
        .find(|c| c["field"] == "notes")
        .unwrap()
        .clone();
    assert_eq!(
        remaining_conflicts(&a, &[notes.clone()])?,
        vec![notes.clone()]
    );
    let before = entity(&a, "overlay", "p").unwrap();
    let mut after = before.clone();
    after["notes"] = json!("Resolved locally");
    write_entity(&mut a, "overlay", "p", Some(&before), Some(&after))?;
    assert!(remaining_conflicts(&a, &[notes])?.is_empty());
    let remaining = remaining_conflicts(&a, &previous)?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["field"], "name");
    assert_eq!(remaining, conflicts(&a)?);
    assert!(remaining_conflicts(&a, &[])?.is_empty());
    Ok(())
}

#[test]
fn existing_winner_resolution_native_delete_survives_validation_and_restart() -> Result<()> {
    let mut a = new_doc("winner-a")?;
    let initial = json!({"id":"p","name":"Initial"});
    write_entity(&mut a, "overlay", "p", None, Some(&initial))?;
    let mut b = load_doc(&a.save(), "winner-b")?;
    write_entity(
        &mut a,
        "overlay",
        "p",
        Some(&initial),
        Some(&json!({"id":"p","name":"A"})),
    )?;
    write_entity(
        &mut b,
        "overlay",
        "p",
        Some(&initial),
        Some(&json!({"id":"p","name":"B"})),
    )?;
    a.merge(&mut b)?;
    let conflict = conflicts(&a)?.remove(0);
    let winner = conflict["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["value"] == conflict["effectiveValue"])
        .unwrap();
    let heads = a.get_heads();
    resolve(
        &mut a,
        &json!({"kind":"overlay","id":"p","field":"name","opId":winner["opId"]}),
    )?;
    let changes = a.get_changes(&heads);
    assert!(changes.iter().any(
        |change| serde_json::to_value(change.decode()).unwrap()["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["action"] == "del")
    ));
    for change in &changes {
        validate_change(change)?;
    }
    assert_eq!(
        changed_fields(&mut a, &heads)?,
        vec![("overlay".into(), "p".into(), "name".into())]
    );
    let a = load_doc(&a.save(), "winner-a")?;
    assert!(conflicts(&a)?.is_empty());
    assert_eq!(
        entity(&a, "overlay", "p").unwrap()["name"],
        conflict["effectiveValue"]["value"]
    );
    let delete =
        json!({"action":"del","obj":"_root","key":key("overlay","p","name"),"pred":["1@aa"]});
    validate_operation(&delete)?;
    let mut payload = delete.clone();
    payload["value"] = json!("unexpected");
    assert!(validate_operation(&payload).is_err());
    let mut private = delete.clone();
    private["key"] = json!(key("overlay", "p", "cookies"));
    assert!(validate_operation(&private).is_err());
    let mut poison = new_doc("private-history")?;
    poison.put(
        ROOT,
        key("product", "p", "cookies"),
        present(&json!("private")).to_string(),
    )?;
    poison.commit();
    poison.delete(ROOT, key("product", "p", "cookies"))?;
    assert!(validate_snapshot(&poison.save()).is_err());
    for path in [
        "",
        "X:\\synthetic\\thumbnail.png",
        "/synthetic.png",
        "\\synthetic.png",
        "../synthetic.png",
        "thumbs/./synthetic.png",
        "thumbs//synthetic.png",
        "thumbs/synthetic\0.png",
    ] {
        assert!(validate_register("overlay", "thumbnailPath", &present(&json!(path))).is_err());
    }
    let mut path_history = new_doc("synthetic-path-history")?;
    path_history.put(
        ROOT,
        key("overlay", "p", "thumbnailPath"),
        present(&json!("X:\\synthetic\\thumbnail.png")).to_string(),
    )?;
    path_history.commit();
    path_history.put(
        ROOT,
        key("overlay", "p", "thumbnailPath"),
        present(&json!("thumbs/safe.png")).to_string(),
    )?;
    assert!(validate_snapshot(&path_history.save()).is_err());
    Ok(())
}

#[test]
fn incremental_projection_matches_full_merge_delete_and_collection_changes() -> Result<()> {
    fn full(doc: &AutoCommit) -> Result<BTreeMap<(String, String), Value>> {
        Ok(entities(doc)?
            .into_iter()
            .map(|(kind, id, value)| ((kind, id), value))
            .collect())
    }
    fn apply(
        doc: &mut AutoCommit,
        heads: &[automerge::ChangeHash],
        rows: &mut BTreeMap<(String, String), Value>,
    ) -> Result<()> {
        let mut grouped: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for (kind, id, field) in changed_fields(doc, heads)? {
            grouped.entry((kind, id)).or_default().push(field);
        }
        for ((kind, id), fields) in grouped {
            let identity = (kind.clone(), id.clone());
            let next = project_entity(doc, &kind, &id, rows.get(&identity).cloned(), &fields)?;
            if let Some(next) = next {
                rows.insert(identity, next);
            } else {
                rows.remove(&identity);
            }
        }
        assert_eq!(*rows, full(doc)?);
        Ok(())
    }
    let mut a = new_doc("a")?;
    let overlay = json!({"id":"p","name":"Before","notes":"Before"});
    let collection = json!({"id":"c","name":"Collection","productIds":["keep"]});
    write_entity(&mut a, "overlay", "p", None, Some(&overlay))?;
    write_entity(&mut a, "collection", "c", None, Some(&collection))?;
    let mut b = load_doc(&a.save(), "b")?;
    write_entity(
        &mut a,
        "overlay",
        "p",
        Some(&overlay),
        Some(&json!({"id":"p","name":"A","notes":"Before"})),
    )?;
    write_entity(
        &mut a,
        "collection",
        "c",
        Some(&collection),
        Some(&json!({"id":"c","name":"Collection","productIds":["keep","a"]})),
    )?;
    let mut rows = full(&a)?;
    let heads = a.get_heads();
    write_entity(
        &mut b,
        "overlay",
        "p",
        Some(&overlay),
        Some(&json!({"id":"p","name":"Before","notes":null})),
    )?;
    write_entity(
        &mut b,
        "collection",
        "c",
        Some(&collection),
        Some(&json!({"id":"c","name":"Collection","productIds":["keep","b"]})),
    )?;
    a.merge(&mut b)?;
    apply(&mut a, &heads, &mut rows)?;
    assert_eq!(
        rows[&("collection".into(), "c".into())]["productIds"],
        json!(["a", "b", "keep"])
    );
    let heads = a.get_heads();
    let before = entity(&a, "collection", "c").unwrap();
    let mut after = before.clone();
    after["productIds"] = json!(["keep"]);
    write_entity(&mut a, "collection", "c", Some(&before), Some(&after))?;
    apply(&mut a, &heads, &mut rows)?;
    let heads = a.get_heads();
    let before = entity(&a, "overlay", "p").unwrap();
    write_entity(&mut a, "overlay", "p", Some(&before), None)?;
    apply(&mut a, &heads, &mut rows)?;
    assert!(project_entity(&a, "overlay", "p", None, &[])?.is_none());
    let heads = a.get_heads();
    let mut recreated = before.clone();
    recreated["notes"] = json!("Recreated note");
    write_entity(&mut a, "overlay", "p", None, Some(&recreated))?;
    apply(&mut a, &heads, &mut rows)?;
    let heads = a.get_heads();
    let before = entity(&a, "collection", "c").unwrap();
    write_entity(&mut a, "collection", "c", Some(&before), None)?;
    apply(&mut a, &heads, &mut rows)?;
    let heads = a.get_heads();
    resolve(
        &mut a,
        &json!({"kind":"collection","id":"c","field":DELETED,"value":false}),
    )?;
    a.commit();
    let before = entity(&a, "collection", "c").unwrap();
    let mut after = before.clone();
    after["name"] = json!("Edited after recovery");
    write_entity(&mut a, "collection", "c", Some(&before), Some(&after))?;
    apply(&mut a, &heads, &mut rows)?;
    let heads = a.get_heads();
    assert!(changed_fields(&mut a, &heads)?.is_empty());
    Ok(())
}

#[test]
fn snapshot_restores_conflicts_fresh_and_causally_preserves_modified_field_intent() -> Result<()> {
    let mut a = new_doc("backup-a")?;
    let original =
        json!({"id":"p","name":"Initial","notes":"Retained","thumbnailPath":"thumbs/initial.png"});
    let hidden = json!({"id":"hidden","name":"Deleted collection","productIds":["p"]});
    write_entity(&mut a, "overlay", "p", None, Some(&original))?;
    write_entity(&mut a, "collection", "hidden", None, Some(&hidden))?;
    let mut b = load_doc(&a.save(), "backup-b")?;
    write_entity(
        &mut a,
        "overlay",
        "p",
        Some(&original),
        Some(&json!({"id":"p","name":"A","notes":"Retained","thumbnailPath":"thumbs/a.png"})),
    )?;
    write_entity(
        &mut b,
        "overlay",
        "p",
        Some(&original),
        Some(&json!({"id":"p","name":"B","notes":"Retained","thumbnailPath":"thumbs/b.png"})),
    )?;
    a.merge(&mut b)?;
    write_entity(&mut a, "collection", "hidden", Some(&hidden), None)?;
    let desired = entities(&a)?;
    let saved_conflicts = conflicts(&a)?;
    assert_eq!(saved_conflicts.len(), 2);
    let bytes = a.save();
    validate_snapshot(&bytes)?;
    let mut fresh = new_doc("fresh-device")?;
    restore_snapshot(&mut fresh, &bytes, &desired)?;
    assert_eq!(entities(&fresh)?, desired);
    assert_eq!(conflicts(&fresh)?, saved_conflicts);
    fresh = load_doc(&fresh.save(), "fresh-device")?;
    assert_eq!(conflicts(&fresh)?, saved_conflicts);
    let name_conflict = saved_conflicts
        .iter()
        .find(|c| c["field"] == "name")
        .unwrap();
    let alternative = name_conflict["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["value"] != name_conflict["effectiveValue"])
        .unwrap();
    resolve(
        &mut fresh,
        &json!({"kind":"overlay","id":"p","field":"name","opId":alternative["opId"]}),
    )?;
    assert_eq!(
        entity(&fresh, "overlay", "p").unwrap()["name"],
        alternative["value"]["value"]
    );

    let mut existing = load_doc(&bytes, "existing-device")?;
    let before = entity(&existing, "overlay", "p").unwrap();
    let mut later = before.clone();
    later["name"] = json!("Later edit");
    later["thumbnailPath"] = json!("thumbs/later.png");
    write_entity(&mut existing, "overlay", "p", Some(&before), Some(&later))?;
    write_entity(
        &mut existing,
        "product",
        "local",
        None,
        Some(&json!({"id":"local","name":"Local extra"})),
    )?;
    let actor = existing.get_actor().clone();
    let mut remapped = desired.clone();
    remapped
        .iter_mut()
        .find(|(k, i, _)| k == "overlay" && i == "p")
        .unwrap()
        .2["thumbnailPath"] = json!("thumbs/restored.png");
    restore_snapshot(&mut existing, &bytes, &remapped)?;
    assert_eq!(*existing.get_actor(), actor);
    assert_eq!(entities(&existing)?, remapped);
    let restored = conflicts(&existing)?;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0]["field"], "name");
    assert_eq!(
        restored[0]["effectiveValue"],
        name_conflict["effectiveValue"]
    );
    let old_choices: BTreeSet<_> = name_conflict["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["value"].to_string())
        .collect();
    let new_choices: BTreeSet<_> = restored[0]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["value"].to_string())
        .collect();
    assert_eq!(old_choices, new_choices);
    resolve(
        &mut existing,
        &json!({"kind":"collection","id":"hidden","field":DELETED,"value":false}),
    )?;
    assert_eq!(entity(&existing, "collection", "hidden").unwrap(), hidden);
    let before = existing.save();
    assert!(validate_snapshot(b"invalid snapshot").is_err());
    assert!(restore_snapshot(&mut existing, b"invalid snapshot", &remapped).is_err());
    assert_eq!(existing.save(), before);
    Ok(())
}
