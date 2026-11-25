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

fn entity(doc: &AutoCommit, kind: &str, id: &str) -> Option<Value> {
    entities(doc)
        .unwrap()
        .into_iter()
        .find(|(k, i, _)| k == kind && i == id)
        .map(|(_, _, v)| v)
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
fn private_fields_and_history_are_rejected() -> Result<()> {
    let mut b = new_doc("b")?;
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
fn existing_winner_put_native_delete_survives_validation_and_restart() -> Result<()> {
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
    let name = key("overlay", "p", "name");
    assert_eq!(register_values(&a, &name)?.len(), 2);
    let winner = effective(&a, &name)?.unwrap();
    let heads = a.get_heads();
    // A restore puts the backup's value over diverged history. When that value is
    // already the winner, Automerge records a native delete of the other value.
    a.put(ROOT, name.as_str(), winner.to_string())?;
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
    let a = load_doc(&a.save(), "winner-a")?;
    assert_eq!(register_values(&a, &name)?.len(), 1);
    assert_eq!(entity(&a, "overlay", "p").unwrap()["name"], winner["value"]);
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
fn snapshot_restores_fresh_and_causally_preserves_modified_field_intent() -> Result<()> {
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
    let bytes = a.save();
    validate_snapshot(&bytes)?;
    let mut fresh = new_doc("fresh-device")?;
    restore_snapshot(&mut fresh, &bytes, &desired)?;
    assert_eq!(entities(&fresh)?, desired);
    fresh = load_doc(&fresh.save(), "fresh-device")?;
    assert_eq!(entities(&fresh)?, desired);

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
    // The deleted collection keeps its retained fields.
    assert!(project(&existing, true)?.contains(&("collection".into(), "hidden".into(), hidden)));
    let before = existing.save();
    assert!(validate_snapshot(b"invalid snapshot").is_err());
    assert!(restore_snapshot(&mut existing, b"invalid snapshot", &remapped).is_err());
    assert_eq!(existing.save(), before);
    Ok(())
}

#[test]
fn snapshot_with_concurrent_values_restores_the_automerge_winner() -> Result<()> {
    let mut a = new_doc("concurrent-a")?;
    let original = json!({"id":"p","name":"Initial","notes":"Kept"});
    write_entity(&mut a, "overlay", "p", None, Some(&original))?;
    let name = key("overlay", "p", "name");
    let mut b = a.fork().with_actor(ActorId::from(b"concurrent-b".to_vec()));
    let mut diverged = a.fork().with_actor(ActorId::from(b"diverged".to_vec()));
    a.put(ROOT, name.as_str(), present(&json!("A")).to_string())?;
    b.put(ROOT, name.as_str(), present(&json!("B")).to_string())?;
    a.merge(&mut b)?;
    assert_eq!(register_values(&a, &name)?.len(), 2);
    let winner = effective(&a, &name)?.unwrap()["value"].clone();
    let desired = entities(&a)?;
    assert_eq!(desired[0].2["name"], winner);
    let bytes = a.save();
    let mut fresh = new_doc("fresh-device")?;
    restore_snapshot(&mut fresh, &bytes, &desired)?;
    diverged.put(ROOT, name.as_str(), present(&json!("C")).to_string())?;
    restore_snapshot(&mut diverged, &bytes, &desired)?;
    for mut restored in [fresh, diverged] {
        assert_eq!(entities(&restored)?, desired);
        let restarted = load_doc(&restored.save(), "restarted-device")?;
        assert_eq!(entities(&restarted)?, desired);
    }
    Ok(())
}
