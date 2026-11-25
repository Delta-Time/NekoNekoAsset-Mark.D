//! Compatible avatars: base identities, aliases, derived rows, pins and defaults.
use super::*;

#[test]
fn source_alias_history_survives_rename_without_reviving_removed_human_aliases() {
    let f = Fixture::new();
    let c = f.open();
    synthetic_named_base(&c, "z-base", "ソラネコ / Oldcat");
    for (id, tag) in [("old-tag", "Oldcat"), ("human-tag", "Humanalias")] {
        c.put("product", id, json!({"id":id,"title":"Synthetic hair","sourceCategory":{"name":"Hair"},"sourceTags":[tag]})).unwrap();
    }
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    synthetic_named_base(&c, "z-base", "ホシネコ / Newcat");
    assert_eq!(
        dynamic_tag_ids(&c, "old-tag"),
        BTreeSet::from(["product:z-base".into()])
    );
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":["Humanalias"]}}}}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&c, "human-tag").len(), 1);
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":[]}}}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&c, "human-tag").is_empty());
    let history =
        c.get("preferences", "app").unwrap().unwrap()["avatarSourceAliases"]["z-base"].clone();
    assert!(strings(&history).contains(&"Oldcat".into()));
    assert!(strings(&history).contains(&"Newcat".into()));
    assert!(!strings(&history).contains(&"Humanalias".into()));
    c.command(
        "overlay",
        json!({"ids":["z-base"],"patch":{"title":"Human nickname"}}),
    )
    .unwrap();
    assert!(!strings(
        &c.get("preferences", "app").unwrap().unwrap()["avatarSourceAliases"]["z-base"]
    )
    .contains(&"Human nickname".into()));

    let restored = Fixture::new();
    let cr = restored.open();
    let manifest = c.manifest().unwrap();
    let saved_preferences = manifest["records"]["preferences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["id"] == "app")
        .unwrap();
    assert_eq!(saved_preferences["avatarSourceAliases"]["z-base"], history);
    cr.restore_manifest(manifest).unwrap();
    assert_eq!(
        cr.get("preferences", "app").unwrap().unwrap()["avatarSourceAliases"]["z-base"],
        history
    );
    assert_eq!(dynamic_tag_ids(&cr, "old-tag").len(), 1);
    assert!(dynamic_tag_ids(&cr, "human-tag").is_empty());
    drop(c);
    let c = f.open();
    assert_eq!(dynamic_tag_ids(&c, "old-tag").len(), 1);
    assert!(dynamic_tag_ids(&c, "human-tag").is_empty());

    synthetic_named_base(&c, "z-collision", "Oldcat");
    assert!(dynamic_tag_ids(&c, "old-tag").is_empty());
    c.delete("product", "z-collision").unwrap();
    assert_eq!(dynamic_tag_ids(&c, "old-tag").len(), 1);
    c.command(
        "overlay",
        json!({"ids":["z-base"],"patch":{"category":"Tool"}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&c, "old-tag").is_empty());
    c.command(
        "overlay",
        json!({"ids":["z-base"],"patch":{},"clearFields":["category"]}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&c, "old-tag").len(), 1);
    c.delete("product", "z-base").unwrap();
    assert!(dynamic_tag_ids(&c, "old-tag").is_empty());
    synthetic_named_base(&c, "different-base", "ホシネコ / Newcat");
    assert!(dynamic_tag_ids(&c, "old-tag").is_empty());
    c.delete("product", "different-base").unwrap();
    synthetic_named_base(&c, "z-base", "ホシネコ / Newcat");
    assert_eq!(dynamic_tag_ids(&c, "old-tag").len(), 1);
    assert!(dynamic_tag_ids(&c, "human-tag").is_empty());
}

#[test]
fn alias_collision_cannot_query_another_canonical_avatars_file_or_manual_evidence() {
    let f = Fixture::new();
    let c = f.open();
    synthetic_named_base(&c, "z-base", "Synthneko");
    for id in [
        "file-compatible",
        "manual-compatible",
        "lowercase-compatible",
    ] {
        product(&c, id);
    }
    c.put("remote_file", "f", json!({"id":"f","productId":"file-compatible","accountId":"a","name":"Mafuyu.unitypackage","revision":"r1"})).unwrap();
    c.put("entitlement", "e", json!({"id":"e","productId":"file-compatible","accountId":"a","fileIds":["f"],"manifestComplete":true})).unwrap();
    c.command(
        "overlay",
        json!({"ids":["manual-compatible"],"patch":{"ownedAvatars":["真冬"]}}),
    )
    .unwrap();
    c.command(
        "overlay",
        json!({"ids":["lowercase-compatible"],"patch":{"ownedAvatars":["mafuyu"]}}),
    )
    .unwrap();
    assert_eq!(
        dynamic_details(&c, "lowercase-compatible")["ownedAvatarEvidence"][0]["avatarId"],
        "booth:5007531"
    );
    c.command("preferences", json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":["Mafuyu","booth:5007531"]}}}})).unwrap();
    let wrong = c
        .command("query", json!({"avatar":"product:z-base"}))
        .unwrap();
    assert_eq!(strings(&wrong["matchedIds"]), vec!["z-base"]);
    let correct = c
        .command("query", json!({"avatar":"booth:5007531"}))
        .unwrap();
    assert_eq!(
        strings(&correct["matchedIds"])
            .into_iter()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "file-compatible".into(),
            "manual-compatible".into(),
            "lowercase-compatible".into()
        ])
    );
}

#[test]
fn dynamic_base_tags_reconcile_creation_category_removal_collision_and_manual_precedence() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product", "a-tagged", json!({"id":"a-tagged","title":"Synthetic clothes","sourceCategory":{"name":"Clothes"},"sourceTags":["Synthneko対応"]})).unwrap();
    assert!(dynamic_tag_ids(&c, "a-tagged").is_empty());
    synthetic_named_base(&c, "z-base", "Synthneko");
    assert_eq!(
        dynamic_tag_ids(&c, "a-tagged"),
        BTreeSet::from(["product:z-base".into()])
    );
    c.command(
        "overlay",
        json!({"ids":["z-base"],"patch":{"category":"Tool"}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&c, "a-tagged").is_empty());
    c.command(
        "overlay",
        json!({"ids":["z-base"],"patch":{},"clearFields":["category"]}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&c, "a-tagged").len(), 1);
    synthetic_named_base(&c, "z-collision", "Synthneko");
    assert!(dynamic_tag_ids(&c, "a-tagged").is_empty());
    c.delete("product", "z-collision").unwrap();
    assert_eq!(dynamic_tag_ids(&c, "a-tagged").len(), 1);
    c.command(
        "overlay",
        json!({"ids":["a-tagged"],"patch":{"ownedAvatars":[]}}),
    )
    .unwrap();
    assert!(dynamic_details(&c, "a-tagged")["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .is_empty());
    c.command(
        "overlay",
        json!({"ids":["a-tagged"],"patch":{},"clearFields":["ownedAvatars"]}),
    )
    .unwrap();
    c.command("preferences", json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":["Astercat"],"englishName":"Astercat"}}}})).unwrap();
    c.command(
        "overlay",
        json!({"ids":["a-tagged"],"patch":{"rejectedTags":["Astercat"]}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&c, "a-tagged").is_empty());
    c.command(
        "overlay",
        json!({"ids":["a-tagged"],"patch":{"tags":["Astercat"],"rejectedTags":[]}}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&c, "a-tagged").len(), 1);
    assert_eq!(
        c.get("overlay", "a-tagged").unwrap().unwrap()["rejectedTags"],
        json!([])
    );
    c.command("overlay", json!({"ids":["a-tagged"],"patch":{"ownedAvatars":["Manualother"],"rejectedTags":["Astercat"]}})).unwrap();
    let p = dynamic_details(&c, "a-tagged");
    assert_eq!(p["ownedAvatars"], json!(["Manualother"]));
    assert!(p["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["source"] == "user"));
    c.command(
        "overlay",
        json!({"ids":["a-tagged"],"patch":{"rejectedTags":[]},"clearFields":["ownedAvatars"]}),
    )
    .unwrap();
    c.delete("product", "z-base").unwrap();
    assert!(dynamic_tag_ids(&c, "a-tagged").is_empty());
    assert!(c
        .get("product", "a-tagged")
        .unwrap()
        .unwrap()
        .get("_tagInputs")
        .is_none());
}

#[test]
fn dynamic_alias_preferences_restart_restore_and_account_scopes_stay_consistent() {
    let a = Fixture::new();
    let restored = Fixture::new();
    let ca = a.open();
    ca.put("product", "a-tagged", json!({"id":"a-tagged","title":"Synthetic hair","sourceCategory":{"name":"Hair"},"sourceTags":["Humanalias"]})).unwrap();
    ca.put(
        "acquisition",
        "a-acquisition",
        json!({"id":"a-acquisition","productId":"a-tagged","accountId":"a"}),
    )
    .unwrap();
    synthetic_named_base(&ca, "z-base", "Synthneko");
    assert!(dynamic_tag_ids(&ca, "a-tagged").is_empty());
    ca.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":["Humanalias"]}}}}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&ca, "a-tagged").len(), 1);
    ca.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":[]}}}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&ca, "a-tagged").is_empty());
    ca.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"product:z-base":{"aliases":["Humanalias"]}}}}),
    )
    .unwrap();
    assert_eq!(dynamic_tag_ids(&ca, "a-tagged").len(), 1);
    for (accounts, expected) in [
        (Value::Null, true),
        (json!(["a"]), true),
        (json!(["b"]), false),
        (json!([]), false),
    ] {
        let query = ca
            .command(
                "query",
                json!({"avatar":"product:z-base","accountIds":accounts}),
            )
            .unwrap();
        assert_eq!(
            strings(&query["matchedIds"]).contains(&"a-tagged".to_owned()),
            expected
        );
    }
    // A source-backed English name works without relying on device-local prefs,
    // so portable restore must independently derive it.
    ca.put("product", "a-tagged", json!({"id":"a-tagged","title":"Synthetic hair","sourceCategory":{"name":"Hair"},"sourceTags":["Synthneko"]})).unwrap();
    let cr = restored.open();
    cr.restore_manifest(ca.manifest().unwrap()).unwrap();
    assert_eq!(dynamic_tag_ids(&cr, "a-tagged").len(), 1);
    drop(ca);
    let ca = a.open();
    assert_eq!(dynamic_tag_ids(&ca, "a-tagged").len(), 1);
}

#[test]
fn materialized_details_track_root_account_and_job_changes_without_losing_dynamic_tags() {
    let f = Fixture::new();
    let c = f.open();
    synthetic_named_base(&c, "base", "Synthneko");
    c.put("product", "p", json!({"id":"p","title":"Synthetic hair","sourceCategory":{"name":"Hair"},"sourceTags":["Synthneko"]})).unwrap();
    c.put(
        "account",
        "a",
        json!({"id":"a","name":"Before","store":"BOOTH"}),
    )
    .unwrap();
    c.put(
        "root",
        "r",
        json!({"id":"r","path":"X:/synthetic","status":"online"}),
    )
    .unwrap();
    c.put("remote_file", "f", json!({"id":"f","productId":"p","accountId":"a","name":"Synthetic.unitypackage","revision":"r1"})).unwrap();
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"p","accountId":"a","fileIds":["f"],"manifestComplete":true}),
    )
    .unwrap();
    c.put("artifact", "f", json!({"id":"f","productId":"p","fileId":"f","revision":"r1","rootId":"r","relativePath":"synthetic/file.unitypackage","status":"ready"})).unwrap();
    c.put(
        "account",
        "a",
        json!({"id":"a","name":"After","store":"BOOTH"}),
    )
    .unwrap();
    c.put(
        "root",
        "r",
        json!({"id":"r","path":"X:/synthetic","status":"offline"}),
    )
    .unwrap();
    let p = dynamic_details(&c, "p");
    assert_eq!(p["sources"][0]["accountName"], "After");
    assert_eq!(p["files"][0]["rootStatus"], "offline");
    assert_eq!(dynamic_tag_ids(&c, "p").len(), 1);
    c.put("job", "j", json!({"id":"j","type":"download","productId":"p","state":"failed","errorCode":"synthetic_failure","updatedAt":"2026-10-09T01:00:00Z"})).unwrap();
    // A stale job failure cannot override all-current verified files.
    assert_eq!(dynamic_details(&c, "p")["status"], "ready");
    let mut artifact = c.get("artifact", "f").unwrap().unwrap();
    artifact["status"] = json!("missing");
    c.put("artifact", "f", artifact.clone()).unwrap();
    assert_eq!(dynamic_details(&c, "p")["status"], "attention");
    assert_eq!(dynamic_tag_ids(&c, "p").len(), 1);
    artifact["status"] = json!("ready");
    c.put("artifact", "f", artifact).unwrap();
    c.delete("job", "j").unwrap();
    assert_eq!(dynamic_details(&c, "p")["status"], "ready");
    assert_eq!(dynamic_tag_ids(&c, "p").len(), 1);
    c.delete("remote_file", "f").unwrap();
    assert_ne!(dynamic_details(&c, "p")["status"], "ready");
    assert_eq!(dynamic_tag_ids(&c, "p").len(), 1);
}

#[test]
fn exact_avatar_tags_remain_global_while_product_and_file_accounts_stay_scoped() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["source-tag", "user-tag", "file-evidence"] {
        c.put("product", id, json!({"id":id,"title":"Synthetic clothes","sourceCategory":{"name":"Clothes"},"sourceTags":if id == "source-tag" { vec!["ミーシェ"] } else { vec![] }})).unwrap();
        let acquisition = format!("a:{id}");
        c.put(
            "acquisition",
            &acquisition,
            json!({"id":acquisition,"productId":id,"accountId":"a"}),
        )
        .unwrap();
    }
    c.command(
        "overlay",
        json!({"ids":["user-tag"],"patch":{"tags":["Mishe"]}}),
    )
    .unwrap();
    c.put("remote_file", "file-b", json!({"id":"file-b","productId":"file-evidence","accountId":"b","name":"Mishe Synthetic.unitypackage","revision":"synthetic-r1"})).unwrap();
    c.put("entitlement", "entitlement-b", json!({"id":"entitlement-b","productId":"file-evidence","accountId":"b","fileIds":["file-b"],"manifestComplete":true})).unwrap();
    for (accounts, expected) in [
        (Value::Null, vec!["file-evidence", "source-tag", "user-tag"]),
        (json!(["a"]), vec!["source-tag", "user-tag"]),
        (json!(["b"]), vec!["file-evidence"]),
        (json!([]), vec![]),
    ] {
        let result = c
            .command(
                "query",
                json!({"avatar":"booth:1256087","accountIds":accounts}),
            )
            .unwrap();
        let actual: BTreeSet<_> = strings(&result["matchedIds"]).into_iter().collect();
        assert_eq!(
            actual,
            expected.into_iter().map(str::to_owned).collect(),
            "accounts={accounts}"
        );
    }
}

#[test]
fn newly_curated_mishe_keeps_exact_product_settings_and_bootstrap_is_idempotent() {
    let f = Fixture::new();
    let c = f.open();
    c.put(
        "product",
        "1256087",
        json!({"id":"1256087","title":"ミーシェ","sourceCategory":{"name":"3Dアバター"}}),
    )
    .unwrap();
    c.put("preferences", "app", json!({"id":"app","avatarSettings":{"booth:1256087":{"enabled":false,"englishName":"Mishe"}}})).unwrap();
    let first = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let entry = first["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "booth:1256087")
        .unwrap();
    assert_eq!(entry["enabled"], false);
    assert_eq!(entry["baseProductIds"], json!(["1256087"]));
    assert!(first["preferences"]["avatarSettings"]
        .get("product:1256087")
        .is_none());
    assert_eq!(
        first["preferences"]["avatarSettings"]["booth:1256087"]["enabled"],
        false
    );
    let revision = c.revision().unwrap();
    let second = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(first["avatarOptions"], second["avatarOptions"]);
    assert_eq!(first["preferences"], second["preferences"]);
    assert_eq!(c.revision().unwrap(), revision);
}

#[test]
#[ignore = "Lead-owned release-only synthetic avatar responsiveness probe"]
fn synthetic_260_base_bootstrap_and_collection_probe() {
    use std::time::Instant;
    let f = Fixture::new();
    let c = f.open();
    let mut records = Vec::new();
    for n in 0..260 {
        let name = format!("Synthcat{n:03}");
        let base = format!("synthetic-base-{n:03}");
        records.push(("product".into(), base.clone(), json!({"id":base,"title":name,"sourceCategory":{"name":"3Dアバター"},"shop":"Synthetic creator"})));
        for item in 0..3 {
            let id = format!("synthetic-accessory-{n:03}-{item}");
            records.push((
                "product".into(),
                id.clone(),
                json!({"id":id,"title":"Synthetic hair","sourceCategory":{"name":"Hair"}}),
            ));
            records.push(("overlay".into(), id.clone(), json!({"id":id,"tags":[name]})));
            for file in 0..2 {
                let fid = format!("{id}:file:{file}");
                records.push(("remote_file".into(), fid.clone(), json!({"id":fid,"productId":id,"name":format!("Synthetic_{file}.unitypackage"),"revision":"synthetic-v1"})));
            }
        }
    }
    c.put_many(records).unwrap();
    let started = Instant::now();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let first = started.elapsed().as_micros();
    let options = boot["avatarOptions"].as_array().unwrap();
    assert_eq!(
        options
            .iter()
            .filter(|entry| text(entry, "id").starts_with("product:"))
            .count(),
        260
    );
    assert!(options
        .iter()
        .filter(|entry| text(entry, "id").starts_with("product:"))
        .all(|entry| entry["matchingProductCount"] == 4));
    let stable = c.get("preferences", "app").unwrap();
    let mut bootstrap_us = Vec::new();
    let mut query_us = Vec::new();
    let mut bytes = 0;
    for _ in 0..5 {
        let started = Instant::now();
        let result = c
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap();
        bytes = serde_json::to_vec(&result).unwrap().len();
        bootstrap_us.push(started.elapsed().as_micros());
        let started = Instant::now();
        let result = c
            .command(
                "query",
                json!({"quick":"catalogue","includeFiles":false,"limit":10000}),
            )
            .unwrap();
        assert_eq!(result["products"].as_array().unwrap().len(), 1040);
        let _ = serde_json::to_vec(&result).unwrap();
        query_us.push(started.elapsed().as_micros());
    }
    assert_eq!(c.get("preferences", "app").unwrap(), stable);
    let started = Instant::now();
    c.command(
        "collection",
        json!({"operation":"add","names":["Synthetic list"],"ids":["synthetic-base-000"]}),
    )
    .unwrap();
    let collection_us = started.elapsed().as_micros();
    let started = Instant::now();
    assert_eq!(
        c.command("collections", json!({})).unwrap()["collections"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let collection_read_us = started.elapsed().as_micros();
    let started = Instant::now();
    c.put("job", "synthetic-probe-job", json!({"id":"synthetic-probe-job","productId":"synthetic-accessory-000-0","type":"download","state":"cancelled"})).unwrap();
    let job_refresh_us = started.elapsed().as_micros();
    assert_eq!(dynamic_tag_ids(&c, "synthetic-accessory-000-0").len(), 1);
    let started = Instant::now();
    synthetic_named_base(&c, "synthetic-base-000", "Renamedsynthcat");
    let base_identity_change_us = started.elapsed().as_micros();
    assert_eq!(dynamic_tag_ids(&c, "synthetic-accessory-000-0").len(), 1);
    c.command(
        "overlay",
        json!({"ids":["synthetic-base-000"],"patch":{"category":"Tool"}}),
    )
    .unwrap();
    assert!(dynamic_tag_ids(&c, "synthetic-accessory-000-0").is_empty());
    println!("synthetic_base_count=260 synthetic_product_count=1040 synthetic_file_count=1560 bootstrap_first_us={first} bootstrap_warm_us={bootstrap_us:?} query_10000_us={query_us:?} bootstrap_serialized_bytes={bytes} collection_write_us={collection_us} collection_read_us={collection_read_us} job_refresh_us={job_refresh_us} base_identity_change_us={base_identity_change_us}");
}

#[test]
fn saved_human_base_name_overrides_ambiguous_source_before_registry_rejection() {
    let f = Fixture::new();
    let c = f.open();
    let source = json!({"id":"synthetic-base","sourceTitle":"星猫 / 海猫","sourceCategory":{"name":"3Dアバター"},"shop":"Synthetic creator"});
    c.put("product", "synthetic-base", source.clone()).unwrap();
    c.command(
        "overlay",
        json!({"ids":["synthetic-base"],"patch":{"note":"Retained human note"}}),
    )
    .unwrap();
    let overlay = c.get("overlay", "synthetic-base").unwrap();
    c.command("preferences",json!({"patch":{"avatarSettings":{"product:synthetic-base":{"originalName":"星空猫","englishName":"Hoshizoraneko","enabled":true}}}})).unwrap();
    for pass in 0..2 {
        let boot = c
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap();
        let options: Vec<_> = boot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["id"] == "product:synthetic-base")
            .collect();
        assert_eq!(options.len(), 1, "{pass}");
        assert_eq!(options[0]["displayName"], "星空猫 / Hoshizoraneko");
        assert_eq!(options[0]["enabled"], true);
        assert_eq!(options[0]["baseProductIds"], json!(["synthetic-base"]));
    }
    drop(c);
    let c = f.open();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let a = boot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "product:synthetic-base")
        .unwrap();
    assert_eq!(a["displayName"], "星空猫 / Hoshizoraneko");
    assert_eq!(a["enabled"], true);
    assert_eq!(
        boot["preferences"]["avatarSettings"]["product:synthetic-base"]["enabled"],
        true
    );
    assert_eq!(c.get("product", "synthetic-base").unwrap(), Some(source));
    assert_eq!(c.get("overlay", "synthetic-base").unwrap(), overlay);
    assert_eq!(
        c.command("product_details", json!({"id":"synthetic-base"}))
            .unwrap()["category"],
        "Base"
    );
}

#[test]
fn ambiguous_slash_bases_remain_selectable_without_fabricated_individual_identities() {
    let f = Fixture::new();
    let c = f.open();
    let sources = [
        (
            "named",
            "VRChat対応 / オリジナル3Dモデル「星空猫」 / VRChat",
        ),
        ("multiple", "星猫 / 海猫"),
        ("platform", "VRChat / 3D Avatar"),
    ];
    for (id, title) in sources {
        c.put(
            "product",
            id,
            json!({"id":id,"sourceTitle":title,"sourceCategory":{"name":"3Dアバター"}}),
        )
        .unwrap();
    }
    product(&c, "manual");
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"ownedAvatars":["星猫 / 海猫"]}}),
    )
    .unwrap();
    let boot = c.command("bootstrap", json!({})).unwrap();
    let options = boot["avatarOptions"].as_array().unwrap();
    let named = options.iter().find(|a| a["id"] == "product:named").unwrap();
    assert_eq!(named["officialName"], "星空猫");
    #[cfg(target_os = "windows")]
    assert_eq!(named["displayName"], "星空猫 / Hoshizoraneko");
    for id in ["product:multiple", "product:platform"] {
        let fallback = options.iter().find(|a| a["id"] == id).unwrap();
        assert_eq!(fallback["nameKind"], "fallback");
        assert_eq!(fallback["englishName"], "");
    }
    assert!(!options.iter().any(|a| text(a, "id").starts_with("avatar:")));
    assert_eq!(boot["products"].as_array().unwrap().len(), 4);
    for (id, title) in sources {
        assert_eq!(c.get("product", id).unwrap().unwrap()["sourceTitle"], title);
    }
    assert_eq!(
        c.get("overlay", "manual").unwrap().unwrap()["ownedAvatars"],
        json!(["星猫 / 海猫"])
    );
}

#[test]
fn avatar_snapshot_is_pure_and_manual_base_visibility_does_not_change_defaults() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product", "manual-base", json!({"id":"manual-base","sourceTitle":"VRChat / 3D Avatar","sourceCategory":{"name":"Clothes"}})).unwrap();
    c.put(
        "job",
        "running-store",
        json!({"id":"running-store","type":"boothSync","state":"running"}),
    )
    .unwrap();
    c.command(
        "overlay",
        json!({"ids":["manual-base"],"patch":{"category":"Base"}}),
    )
    .unwrap();
    let preferences = c.get("preferences", "app").unwrap();
    let revision = c.revision().unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    let option = snapshot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["id"] == "product:manual-base")
        .unwrap();
    assert_eq!(option["nameKind"], "fallback");
    assert_eq!(option["enabled"], false);
    assert_eq!(option["baseProductIds"], json!(["manual-base"]));
    assert_eq!(c.get("preferences", "app").unwrap(), preferences);
    assert_eq!(c.revision().unwrap(), revision);
    let bootstrap = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(snapshot["avatarOptions"], bootstrap["avatarOptions"]);
    product(&c, "selected-fallback");
    c.command(
        "overlay",
        json!({"ids":["selected-fallback"],"patch":{"ownedAvatars":["product:manual-base"]}}),
    )
    .unwrap();
    let selected = dynamic_details(&c, "selected-fallback");
    assert_eq!(
        selected["ownedAvatarEvidence"],
        json!([{"avatarId":"product:manual-base","avatar":option["displayName"],"source":"user"}])
    );
    c.put("product", "generic-tag", json!({"id":"generic-tag","sourceTitle":"Authored clothing","sourceCategory":{"name":"Clothes"},"sourceTags":["VRChat / 3D Avatar"]})).unwrap();
    assert!(dynamic_tag_ids(&c, "generic-tag").is_empty());
    assert_eq!(
        c.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"],
        preferences.unwrap()["avatarAutomaticEnabled"]
    );
}

#[test]
fn avatar_presentation_overrides_survive_source_refresh_without_becoming_tag_evidence() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product", "named-base", json!({"id":"named-base","sourceTitle":"シロネコ","shop":"Authored source shop","sourceCategory":{"name":"3Dアバター"}})).unwrap();
    for (id, tag) in [
        ("source-tag", "Shironeko"),
        ("presentation-tag", "Custom display"),
    ] {
        c.put("product", id, json!({"id":id,"sourceTitle":"Authored clothing","sourceCategory":{"name":"Clothes"},"sourceTags":[tag]})).unwrap();
    }
    c.command("preferences", json!({"patch":{"avatarSettings":{"product:named-base":{"displayName":"Custom display","shopName":"Custom group","enabled":false}},"avatarAliases":{"custom display":"product:named-base"}}})).unwrap();
    product(&c, "legacy-display");
    c.command(
        "overlay",
        json!({"ids":["legacy-display"],"patch":{"ownedAvatars":["Custom display"]}}),
    )
    .unwrap();
    let option = |catalogue: &Catalogue| {
        catalogue.command("avatar_options", json!({})).unwrap()["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "product:named-base")
            .unwrap()
            .clone()
    };
    let first = option(&c);
    assert_eq!(first["displayName"], "Custom display");
    assert_eq!(first["shopName"], "Custom group");
    assert_eq!(first["sourceDisplayName"], "シロネコ / Shironeko");
    assert_eq!(first["sourceShopName"], "Authored source shop");
    assert_eq!(first["matchingProductCount"], 2);
    assert!(!strings(&first["aliases"]).contains(&"Custom display".into()));
    assert_eq!(
        dynamic_tag_ids(&c, "source-tag"),
        BTreeSet::from(["product:named-base".into()])
    );
    assert!(dynamic_tag_ids(&c, "presentation-tag").is_empty());
    c.put("product", "named-base", json!({"id":"named-base","sourceTitle":"クロネコ","shop":"Refreshed source shop","sourceCategory":{"name":"3Dアバター"}})).unwrap();
    assert_eq!(option(&c)["displayName"], "Custom display");
    assert_eq!(option(&c)["sourceDisplayName"], "クロネコ / Kuroneko");
    assert_eq!(option(&c)["sourceShopName"], "Refreshed source shop");
    drop(c);
    let c = f.open();
    assert_eq!(option(&c)["displayName"], "Custom display");
    assert_eq!(option(&c)["shopName"], "Custom group");
    c.command("preferences", json!({"patch":{"avatarSettings":{"product:named-base":{"displayName":"  ","shopName":"","enabled":false}}}})).unwrap();
    assert_eq!(option(&c)["displayName"], "クロネコ / Kuroneko");
    assert_eq!(option(&c)["shopName"], "Refreshed source shop");
    for invalid in [
        json!({"displayName":true}),
        json!({"shopName":[]}),
        json!({"enabled":"false"}),
    ] {
        let before = c.get("preferences", "app").unwrap();
        assert!(c
            .command(
                "preferences",
                json!({"patch":{"avatarSettings":{"product:named-base":invalid}}})
            )
            .is_err());
        assert_eq!(c.get("preferences", "app").unwrap(), before);
    }
}

#[test]
fn avatar_delta_uses_current_exact_evidence_preserves_legacy_and_undoes_atomically() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["base-a", "base-b"] {
        synthetic_named_base(&c, id, "Sharedcat");
    }
    product(&c, "edited");
    // Represents a newer edit committed after the popup captured its draft.
    c.command("overlay", json!({"ids":["edited"],"patch":{"ownedAvatars":["product:base-a","product:base-b","  Legacy label  "]}})).unwrap();
    let before = c.get("overlay", "edited").unwrap();
    let result = c.command("overlay", json!({"changes":[{"id":"edited","patch":{"note":"Changed together"},"ownedAvatarsDelta":{"add":["Mafuyu","booth:5007531"],"remove":["product:base-a"]}}]})).unwrap();
    let overlay = c.get("overlay", "edited").unwrap().unwrap();
    assert_eq!(
        strings(&overlay["ownedAvatars"])
            .into_iter()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "product:base-b".into(),
            "  Legacy label  ".into(),
            "Mafuyu".into()
        ])
    );
    let evidence: BTreeSet<_> = dynamic_details(&c, "edited")["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| text(entry, "avatarId").to_owned())
        .collect();
    assert!(evidence.contains("product:base-b"));
    assert!(!evidence.contains("product:base-a"));
    assert_eq!(overlay["note"], "Changed together");
    c.command("undo", json!({"token":result["undoToken"]}))
        .unwrap();
    assert_eq!(c.get("overlay", "edited").unwrap(), before);
    c.command("overlay", json!({"ids":["edited"],"patch":{},"ownedAvatarsDelta":{"add":[],"remove":["product:base-a","product:base-b","  Legacy label  "]}})).unwrap();
    assert_eq!(
        c.get("overlay", "edited").unwrap().unwrap()["ownedAvatars"],
        json!([])
    );
    c.put("product", "edited", json!({"id":"edited","sourceTitle":"Refreshed clothing","sourceCategory":{"name":"Clothes"},"sourceTags":["Mafuyu"]})).unwrap();
    assert_eq!(
        dynamic_details(&c, "edited")["ownedAvatarEvidence"],
        json!([])
    );
}

#[test]
fn avatar_delta_rejects_conflicts_and_rolls_back_all_changes_for_missing_identity() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "one");
    product(&c, "two");
    for change in [
        json!({"id":"one","patch":{"ownedAvatars":[]},"ownedAvatarsDelta":{"add":[],"remove":[]}}),
        json!({"id":"one","patch":{},"ownedAvatarsDelta":{"add":["Mafuyu"],"remove":["Mafuyu"]}}),
        json!({"id":"one","patch":{},"ownedAvatarsDelta":{"add":[true],"remove":[]}}),
    ] {
        assert!(c.command("overlay", json!({"changes":[change]})).is_err());
    }
    assert!(c.command("overlay", json!({"changes":[
        {"id":"one","patch":{"note":"Must roll back"},"ownedAvatarsDelta":{"add":["booth:5007531"],"remove":[]}},
        {"id":"two","patch":{},"ownedAvatarsDelta":{"add":["product:missing"],"remove":[]}}
    ]})).is_err());
    assert!(c.get("overlay", "one").unwrap().is_none());
    assert!(c.get("overlay", "two").unwrap().is_none());
    assert!(dynamic_tag_ids(&c, "one").is_empty());
}

#[test]
fn avatar_delta_can_add_existing_evidence_only_choice_without_storing_its_derived_id() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["origin", "target"] {
        product(&c, id);
    }
    c.command(
        "overlay",
        json!({"ids":["origin"],"patch":{"ownedAvatars":["ソラネコ / Skycat"]}}),
    )
    .unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    let option = snapshot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| text(option, "id").starts_with("avatar:"))
        .unwrap();
    c.command(
        "overlay",
        json!({"ids":["target"],"ownedAvatarsDelta":{"add":[option["id"]],"remove":[]}}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "target").unwrap().unwrap()["ownedAvatars"],
        json!(["ソラネコ / Skycat"])
    );
    let after = c.command("avatar_options", json!({})).unwrap();
    assert_eq!(
        after["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == option["id"])
            .unwrap()["matchingProductCount"],
        2
    );
}

#[test]
fn avatar_delta_preserves_wrapped_derived_identity_before_origin_and_after_origin_deletion() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let c = f.open();
    for id in ["a-target", "z-origin"] {
        product(&c, id);
    }
    let label = "【無料】Ren♡Ai";
    let (identity_name, _) = crate::taxonomy::avatar_label_parts(label);
    let id = format!(
        "avatar:{:x}",
        Sha256::digest(normalized(&identity_name).as_bytes())
    );
    c.command(
        "overlay",
        json!({"ids":["z-origin"],"patch":{"ownedAvatars":[label]}}),
    )
    .unwrap();
    c.command("preferences", json!({"patch":{"avatarSettings":{(id.clone()):{"displayName":"Saved Ren name","shopName":"Saved Ren group","enabled":true}}}})).unwrap();
    let read_snapshot = |catalogue: &Catalogue| {
        let preferences = catalogue.get("preferences", "app").unwrap();
        let revision = catalogue.revision().unwrap();
        let snapshot = catalogue.command("avatar_options", json!({})).unwrap();
        assert_eq!(catalogue.get("preferences", "app").unwrap(), preferences);
        assert_eq!(catalogue.revision().unwrap(), revision);
        let derived: Vec<_> = snapshot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| text(entry, "id").starts_with("avatar:"))
            .cloned()
            .collect();
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0]["id"], id);
        assert_eq!(derived[0]["officialName"], "Ren♡Ai");
        assert_eq!(derived[0]["sourceIdentityValue"], label);
        assert_eq!(derived[0]["displayName"], "Saved Ren name");
        assert_eq!(derived[0]["shopName"], "Saved Ren group");
        assert_eq!(derived[0]["enabled"], true);
        derived[0].clone()
    };
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 1);
    c.command(
        "overlay",
        json!({"ids":["a-target"],"ownedAvatarsDelta":{"add":[id.clone()],"remove":[]}}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "a-target").unwrap().unwrap()["ownedAvatars"],
        json!([label])
    );
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 2);
    c.delete("product", "z-origin").unwrap();
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 1);
    assert!(
        c.get("preferences", "app").unwrap().unwrap()["avatarSettings"]
            .get(&id)
            .is_some()
    );
    drop(c);
    let c = f.open();
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 1);
}

#[test]
fn avatar_single_overlay_keeps_raw_derived_identity_after_origin_deletion_and_reopen() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let c = f.open();
    for id in ["a-target", "z-origin"] {
        product(&c, id);
    }
    let label = "【無料】Ren♡Ai";
    let (identity_name, _) = crate::taxonomy::avatar_label_parts(label);
    let id = format!(
        "avatar:{:x}",
        Sha256::digest(normalized(&identity_name).as_bytes())
    );
    c.command(
        "overlay",
        json!({"ids":["z-origin"],"patch":{"ownedAvatars":[label]}}),
    )
    .unwrap();
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{(id.clone()):{
            "displayName":"Saved single Ren name","shopName":"Saved single Ren group","enabled":true
        }}}}),
    )
    .unwrap();
    let read_snapshot = |catalogue: &Catalogue| {
        let preferences = catalogue.get("preferences", "app").unwrap();
        let revision = catalogue.revision().unwrap();
        let snapshot = catalogue.command("avatar_options", json!({})).unwrap();
        assert_eq!(catalogue.get("preferences", "app").unwrap(), preferences);
        assert_eq!(catalogue.revision().unwrap(), revision);
        let derived: Vec<_> = snapshot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| text(entry, "id").starts_with("avatar:"))
            .cloned()
            .collect();
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0]["id"], id);
        assert_eq!(derived[0]["officialName"], "Ren♡Ai");
        assert_eq!(derived[0]["sourceDisplayName"], "Ren♡Ai");
        assert_eq!(derived[0]["sourceIdentityValue"], label);
        assert_eq!(derived[0]["displayName"], "Saved single Ren name");
        assert_eq!(derived[0]["shopName"], "Saved single Ren group");
        assert_eq!(derived[0]["enabled"], true);
        derived[0].clone()
    };
    let origin = read_snapshot(&c);
    assert_eq!(origin["matchingProductCount"], 1);
    // Single Edit sends the raw representative from the selected option through
    // the existing exact overlay patch, rather than the runtime hash ID.
    c.command(
        "overlay",
        json!({"ids":["a-target"],"patch":{
            "ownedAvatars":[origin["sourceIdentityValue"]]
        }}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "a-target").unwrap().unwrap()["ownedAvatars"],
        json!([label])
    );
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 2);
    c.delete("product", "z-origin").unwrap();
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 1);
    assert!(
        c.get("preferences", "app").unwrap().unwrap()["avatarSettings"]
            .get(&id)
            .is_some()
    );
    drop(c);
    let c = f.open();
    assert_eq!(read_snapshot(&c)["matchingProductCount"], 1);
}

#[test]
fn avatar_label_cleanup_keeps_existing_derived_identity_settings() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let c = f.open();
    product(&c, "label-owner");
    let label = "【無料】Syntheticcat";
    let (identity_name, _) = crate::taxonomy::avatar_label_parts(label);
    let id = format!(
        "avatar:{:x}",
        Sha256::digest(normalized(&identity_name).as_bytes())
    );
    c.command(
        "overlay",
        json!({"ids":["label-owner"],"patch":{"ownedAvatars":[label]}}),
    )
    .unwrap();
    c.command("preferences", json!({"patch":{"avatarSettings":{(id.clone()):{"displayName":"Saved presentation","enabled":false}}}})).unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    let option = snapshot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["id"] == id)
        .unwrap();
    assert_eq!(option["officialName"], "Syntheticcat");
    assert_eq!(option["sourceDisplayName"], "Syntheticcat");
    assert_eq!(option["displayName"], "Saved presentation");
    assert_eq!(option["enabled"], false);
    assert!(!strings(&option["aliases"]).contains(&"Saved presentation".into()));
}

#[test]
fn avatar_uncertain_base_ignores_historical_text_aliases_without_erasing_them() {
    let f = Fixture::new();
    let c = f.open();
    synthetic_named_base(&c, "uncertain-base", "星猫 / 海猫");
    c.command("preferences", json!({"patch":{"avatarAliases":{"legacy guessed":"product:uncertain-base","product:uncertain-base":"product:uncertain-base"}}})).unwrap();
    for n in 0..6 {
        let id = format!("legacy-member-{n}");
        product(&c, &id);
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"ownedAvatars":["Legacy guessed"]}}),
        )
        .unwrap();
    }
    let before = c.get("preferences", "app").unwrap().unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    let uncertain = snapshot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "product:uncertain-base")
        .unwrap();
    assert_eq!(uncertain["matchingProductCount"], 1);
    assert_eq!(uncertain["enabled"], false);
    assert_eq!(c.get("preferences", "app").unwrap().unwrap(), before);
    c.refresh_avatar_defaults().unwrap();
    let preferences = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(
        preferences["avatarAliases"]["legacy guessed"],
        "product:uncertain-base"
    );
    assert_eq!(
        preferences["avatarAliases"]["product:uncertain-base"],
        "product:uncertain-base"
    );
    assert!(preferences["avatarAutomaticEnabled"]
        .get("product:uncertain-base")
        .is_none());
    c.command(
        "overlay",
        json!({"ids":["legacy-member-0"],"patch":{"ownedAvatars":["product:uncertain-base"]}}),
    )
    .unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    assert_eq!(
        snapshot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "product:uncertain-base")
            .unwrap()["matchingProductCount"],
        2
    );
}

#[test]
fn avatar_legacy_human_name_romanizes_corrected_stem_instead_of_uncertain_source() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let c = f.open();
    product(&c, "legacy-owner");
    let source_label = "【 R18 Edition】苺佳(ichica) and GLC and 蒼葉(aoba)";
    let (identity_name, _) = crate::taxonomy::avatar_label_parts(source_label);
    let id = format!(
        "avatar:{:x}",
        Sha256::digest(normalized(&identity_name).as_bytes())
    );
    let source_key = crate::taxonomy::canonical_tag(source_label);
    c.command(
        "overlay",
        json!({"ids":["legacy-owner"],"patch":{"ownedAvatars":[source_label]}}),
    )
    .unwrap();
    c.command(
        "preferences",
        json!({"patch":{
            "avatarAliases":{(source_key):id.clone()},
            "avatarSettings":{(id.clone()):{"originalName":"シロネコ","enabled":false}}
        }}),
    )
    .unwrap();
    let snapshot = c.command("avatar_options", json!({})).unwrap();
    let option = snapshot["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["id"] == id)
        .unwrap();
    assert_eq!(option["officialName"], "シロネコ");
    assert_eq!(option["englishName"], "Shironeko");
    assert_eq!(option["nameKind"], "single");
    assert_eq!(
        c.get("overlay", "legacy-owner").unwrap().unwrap()["ownedAvatars"],
        json!([source_label])
    );
}

#[test]
fn dynamic_bilingual_aliases_deduplicate_independent_of_order_and_restart() {
    let mut ids = vec![];
    for labels in [
        json!(["ソラネコ / Skycat", "ソラネコ"]),
        json!(["ソラネコ", "ソラネコ / Skycat"]),
    ] {
        let f = Fixture::new();
        let c = f.open();
        product(&c, "synthetic");
        c.command(
            "overlay",
            json!({"ids":["synthetic"],"patch":{"ownedAvatars":labels}}),
        )
        .unwrap();
        let boot = c
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap();
        let options: Vec<_> = boot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| text(a, "id").starts_with("avatar:"))
            .collect();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0]["matchingProductCount"], 1);
        assert_eq!(options[0]["displayName"], "ソラネコ / Skycat");
        let id = options[0]["id"].clone();
        ids.push(id.clone());
        let revision = c.revision().unwrap();
        c.command("bootstrap", json!({"includeProducts":false}))
            .unwrap();
        assert_eq!(c.revision().unwrap(), revision);
        drop(c);
        let c = f.open();
        let again = c
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap();
        let options: Vec<_> = again["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| text(a, "id").starts_with("avatar:"))
            .collect();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0]["id"], id);
        assert_eq!(
            c.command("query", json!({"avatar":"Skycat"})).unwrap()["matched"],
            1
        );
    }
    assert_eq!(ids[0], ids[1]);
}

#[test]
fn derived_avatar_options_filter_the_products_they_count() {
    let f = Fixture::new();
    let c = f.open();
    let clothing = |id: &str| json!({"id":id,"store":"BOOTH","sourceTitle":"Authored VRChat clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"]});
    c.put("account", "a", json!({"id":"a","store":"BOOTH"}))
        .unwrap();
    c.put("product", "early", clothing("early")).unwrap();
    // Unlisted Base names arrive raw from bound variants and manual choices.
    for (id, variant) in [
        ("900101", "SyntheticNeko専用"),
        ("900102", "ＳｙｎｔｈｅｔｉｃＮｅｋｏ専用"),
        ("900103", "【無料】Ren♡Ai専用"),
        ("900104", "シロネコ専用"),
    ] {
        let mut source = clothing(id);
        source["publicVariants"] =
            json!([{"id":format!("{id}-v"),"type":"digital","name":variant}]);
        c.put_many(vec![
            ("product".into(), id.into(), source),
            ("remote_file".into(), format!("{id}-f"), json!({"id":format!("{id}-f"),"productId":id,"accountId":"a","variantId":format!("{id}-v"),"name":"opaque.zip","revision":"r1"})),
            ("entitlement".into(), format!("{id}-e"), json!({"id":format!("{id}-e"),"productId":id,"accountId":"a","fileIds":[format!("{id}-f")],"manifestComplete":true})),
        ]).unwrap();
    }
    c.put("product", "manual", clothing("manual")).unwrap();
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"ownedAvatars":["SYNTHETICNEKO"]}}),
    )
    .unwrap();
    let check = |c: &Catalogue, ren: u64| {
        let options = c.command("avatar_options", json!({})).unwrap()["avatarOptions"]
            .as_array()
            .unwrap()
            .clone();
        let mut derived: Vec<_> = options
            .iter()
            .filter(|o| text(o, "id").starts_with("avatar:"))
            .map(|option| {
                let matched =
                    c.command("query", json!({"avatar":option["id"]})).unwrap()["matched"].clone();
                (
                    text(option, "officialName").to_owned(),
                    option["matchingProductCount"].clone(),
                    matched,
                )
            })
            .collect();
        derived.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            derived,
            [
                ("Ren♡Ai".to_owned(), json!(ren), json!(ren)),
                ("SyntheticNeko".to_owned(), json!(3), json!(3)),
                ("シロネコ".to_owned(), json!(1), json!(1)),
            ]
        );
        let synthetic = options
            .iter()
            .find(|o| o["officialName"] == "SyntheticNeko")
            .unwrap();
        let mut scoped = strings(
            &c.command(
                "query",
                json!({"avatar":synthetic["id"],"accountIds":["a"],"includeOffline":false}),
            )
            .unwrap()["matchedIds"],
        );
        scoped.sort();
        assert_eq!(scoped, ["900101", "900102"]);
    };
    // New names match before bootstrap persists their aliases.
    check(&c, 1);
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    // A later cleaned spelling joins the existing derived identity.
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"ownedAvatars":["SYNTHETICNEKO", "Ren♡Ai"]}}),
    )
    .unwrap();
    check(&c, 2);
    // A saved alias links an ambiguous label on an earlier product; it counts
    // regardless of whether that product precedes the identity's first name.
    let label = "Ren♡Ai and SyntheticNeko and シロネコ";
    assert!(
        !crate::taxonomy::avatar_label_is_unambiguous(label)
            || crate::taxonomy::parsed_avatar_label(label).name_kind != "single"
    );
    let options = c.command("avatar_options", json!({})).unwrap()["avatarOptions"].clone();
    let ren = options
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["officialName"] == "Ren♡Ai")
        .unwrap()["id"]
        .clone();
    let mut aliases = c.get("preferences", "app").unwrap().unwrap()["avatarAliases"].clone();
    aliases[crate::taxonomy::canonical_tag(label)] = ren;
    c.command("preferences", json!({"patch":{"avatarAliases":aliases}}))
        .unwrap();
    c.command(
        "overlay",
        json!({"ids":["early"],"patch":{"ownedAvatars":[label]}}),
    )
    .unwrap();
    check(&c, 3);
    drop(c);
    check(&f.open(), 3);
}

#[test]
fn item_links_credit_an_owned_base_without_their_labels_becoming_names() {
    let f = Fixture::new();
    let c = f.open();
    c.put("account", "a", json!({"id":"a","store":"BOOTH"}))
        .unwrap();
    synthetic_named_base(&c, "900201", "ソラネコ");
    // Sellers label links with a shop or several avatars as well as a name.
    let list = "ソラネコ and ウミネコ and ホシネコ";
    assert!(
        !crate::taxonomy::avatar_label_is_unambiguous(list)
            || crate::taxonomy::parsed_avatar_label(list).name_kind != "single"
    );
    citing_product(&c, "cites-shop", "900201", "Authored Shop");
    citing_product(&c, "cites-list", "900201", list);
    citing_product(&c, "cites-other", "900202", "ウミネコ");
    // A variant's bare name has no ID either; its raw spelling reaches the filter too.
    owned_variant_product(&c, "names-base", "Soraneko専用", json!([]));
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(avatar_rows(&c, "product:")[0]["matchingProductCount"], 4);
    assert_eq!(
        avatar_filter(&c, &json!("product:900201")),
        ["900201", "cites-list", "cites-shop", "names-base"]
    );
    // A link to a Base outside this library keeps its derived row.
    let derived = avatar_rows(&c, "avatar:");
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0]["officialName"], "ウミネコ");
    assert_eq!(derived[0]["matchingProductCount"], 1);
    assert_eq!(avatar_filter(&c, &derived[0]["id"]), ["cites-other"]);
    let aliases = c.get("preferences", "app").unwrap().unwrap()["avatarAliases"].clone();
    assert_eq!(aliases["booth:900202"], derived[0]["id"]);
    assert!(aliases.get("booth:900201").is_none());
    assert!(aliases
        .as_object()
        .unwrap()
        .keys()
        .all(|alias| !alias.contains("authored shop") && !alias.contains(" and ")));
    // Batch Edit acts on the Base these products count toward.
    c.command(
        "overlay",
        json!({"ids":["cites-shop"],"ownedAvatarsDelta":{"add":[],"remove":["product:900201"]}}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "cites-shop").unwrap().unwrap()["ownedAvatars"],
        json!([])
    );
    c.command(
        "overlay",
        json!({"ids":["cites-list"],"ownedAvatarsDelta":{"add":[derived[0]["id"]],"remove":[]}}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "cites-list").unwrap().unwrap()["ownedAvatars"],
        json!(["product:900201", "ウミネコ"])
    );
    assert_eq!(
        avatar_filter(&c, &json!("product:900201")),
        ["900201", "cites-list", "names-base"]
    );
}

#[test]
fn a_links_derived_row_hands_over_to_its_base_and_returns_if_the_base_leaves() {
    let f = Fixture::new();
    let c = f.open();
    c.put("account", "a", json!({"id":"a","store":"BOOTH"}))
        .unwrap();
    citing_product(&c, "cites-label", "900201", "✦ソラネコ様");
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let row = avatar_rows(&c, "avatar:")[0].clone();
    let id = row["id"].clone();
    let key = text(&row, "id").to_owned();
    // Everything a human can have saved against that row.
    product(&c, "manual");
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"ownedAvatars":[row["sourceIdentityValue"]]}}),
    )
    .unwrap();
    let saved = json!({"enabled":true,"displayName":"Saved name","shopName":"Saved shop"});
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{(key.clone()):saved}}}),
    )
    .unwrap();
    assert_eq!(avatar_rows(&c, "avatar:")[0]["matchingProductCount"], 2);

    synthetic_named_base(&c, "900201", "ソラネコ");
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert!(avatar_rows(&c, "avatar:").is_empty());
    let base = avatar_rows(&c, "product:")[0].clone();
    assert_eq!(base["matchingProductCount"], 3);
    assert_eq!(base["displayName"], "Saved name");
    assert_eq!(base["shopName"], "Saved shop");
    assert_eq!(base["enabled"], true);
    assert_eq!(base["explicitEnabled"], true);
    assert!(strings(&base["aliases"]).contains(&key));
    // Edit and Batch Edit find the manual choice's text through the Base.
    assert!(strings(&base["aliases"])
        .iter()
        .any(|alias| alias == text(&row, "sourceIdentityValue")));
    let members = ["900201", "cites-label", "manual"];
    assert_eq!(avatar_filter(&c, &base["id"]), members);
    // A saved search still holds the row's ID.
    assert_eq!(avatar_filter(&c, &id), members);
    let preferences = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(preferences["avatarSettings"], json!({(key.clone()):saved}));
    assert_eq!(preferences["avatarAliases"]["booth:900201"], id);
    assert_eq!(
        c.get("overlay", "manual").unwrap().unwrap()["ownedAvatars"],
        json!([row["sourceIdentityValue"]])
    );
    let revision = c.revision().unwrap();
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(c.revision().unwrap(), revision);

    // The Base's own choices win, including a cleared name.
    c.command("preferences", json!({"patch":{"avatarSettings":{(key.clone()):saved,"product:900201":{"enabled":false,"displayName":""}}}})).unwrap();
    let base = avatar_rows(&c, "product:")[0].clone();
    assert_eq!(base["enabled"], false);
    assert_eq!(base["displayName"], base["sourceDisplayName"]);
    assert_eq!(base["shopName"], "Saved shop");
    // Unticking a duplicate never hides the Base.
    c.command("preferences", json!({"patch":{"avatarSettings":{(key.clone()):{"enabled":false}},"avatarAutomaticEnabled":{"product:900201":true}}})).unwrap();
    assert_eq!(avatar_rows(&c, "product:")[0]["enabled"], true);

    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{(key.clone()):saved},"avatarAutomaticEnabled":{}}}),
    )
    .unwrap();
    c.delete("product", "900201").unwrap();
    let row = avatar_rows(&c, "avatar:")[0].clone();
    assert_eq!(row["id"], id);
    assert_eq!(row["matchingProductCount"], 2);
    assert_eq!(row["displayName"], "Saved name");
    assert_eq!(row["enabled"], true);
    assert_eq!(avatar_filter(&c, &id), ["cites-label", "manual"]);
}

#[test]
fn a_label_shared_with_another_link_keeps_its_derived_row_apart_from_the_base() {
    let f = Fixture::new();
    let c = f.open();
    c.put("account", "a", json!({"id":"a","store":"BOOTH"}))
        .unwrap();
    citing_product(&c, "cites-owned", "900201", "✦ソラネコ様");
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let id = avatar_rows(&c, "avatar:")[0]["id"].clone();
    let key = text(&avatar_rows(&c, "avatar:")[0], "id").to_owned();
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{(key):{"displayName":"Saved name"}}}}),
    )
    .unwrap();
    synthetic_named_base(&c, "900201", "ソラネコ");
    citing_product(&c, "cites-other", "900203", "✦ソラネコ様");
    let check = |c: &Catalogue| {
        let derived = avatar_rows(c, "avatar:");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0]["id"], id);
        assert_eq!(derived[0]["matchingProductCount"], 1);
        assert_eq!(derived[0]["displayName"], "Saved name");
        assert_eq!(
            avatar_filter(c, &json!("product:900201")),
            ["900201", "cites-owned"]
        );
    };
    check(&c);
    // Its filter joins the row once bootstrap has recorded the second link.
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    check(&c);
    assert_eq!(avatar_filter(&c, &id), ["cites-other"]);
}

#[test]
fn a_bare_variant_name_counts_toward_the_one_identity_with_that_name() {
    let f = Fixture::new();
    let c = f.open();
    c.put("account", "a", json!({"id":"a","store":"BOOTH"}))
        .unwrap();
    owned_variant_product(&c, "names-base", "ソラネコ専用", json!([]));
    owned_variant_product(&c, "untick", "ソラネコ専用", json!([]));
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let row = avatar_rows(&c, "avatar:")[0].clone();
    let id = row["id"].clone();
    let key = text(&row, "id").to_owned();
    // The label's spelling is our romanization, not one the Base's source uses.
    assert_eq!(row["displayName"], "ソラネコ / Soraneko");
    let saved = json!({"enabled":true,"displayName":"Saved name"});
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{(key.clone()):saved}}}),
    )
    .unwrap();
    product(&c, "manual");
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"ownedAvatars":[row["sourceIdentityValue"]]}}),
    )
    .unwrap();
    assert_eq!(avatar_rows(&c, "avatar:")[0]["matchingProductCount"], 3);

    synthetic_named_base(&c, "900201", "オリジナル3Dモデル「ソラネコ -Starcat-」");
    let before = c.get("preferences", "app").unwrap().unwrap();
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert!(avatar_rows(&c, "avatar:").is_empty());
    let base = avatar_rows(&c, "product:")[0].clone();
    assert_eq!(base["sourceDisplayName"], "ソラネコ / Starcat");
    assert_eq!(base["displayName"], "Saved name");
    assert_eq!(base["enabled"], true);
    assert_eq!(base["matchingProductCount"], 4);
    assert!(strings(&base["aliases"]).contains(&key));
    assert!(strings(&base["aliases"])
        .iter()
        .any(|alias| alias == "ソラネコ / Soraneko"));
    let members = ["900201", "manual", "names-base", "untick"];
    assert_eq!(avatar_filter(&c, &base["id"]), members);
    // A saved search still holds the row's ID.
    assert_eq!(avatar_filter(&c, &id), members);
    // Everything saved stays as it was, so the row can come back.
    let preferences = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(preferences["avatarSettings"], before["avatarSettings"]);
    assert_eq!(preferences["avatarAliases"]["ソラネコ / soraneko"], id);
    assert_eq!(
        c.get("overlay", "manual").unwrap().unwrap()["ownedAvatars"],
        json!([row["sourceIdentityValue"]])
    );
    let revision = c.revision().unwrap();
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(c.revision().unwrap(), revision);
    // Batch Edit reaches the label through the Base it counts toward.
    c.command(
        "overlay",
        json!({"ids":["untick"],"ownedAvatarsDelta":{"add":[],"remove":["product:900201"]}}),
    )
    .unwrap();
    assert_eq!(
        c.get("overlay", "untick").unwrap().unwrap()["ownedAvatars"],
        json!([])
    );
    let members = ["900201", "manual", "names-base"];
    assert_eq!(avatar_filter(&c, &base["id"]), members);

    // A special edition leaves the plain product's name its own.
    synthetic_named_base(
        &c,
        "900202",
        "オリジナル3Dモデル「ソラネコ -Starcat-」Gen3.0",
    );
    assert!(avatar_rows(&c, "avatar:").is_empty());
    assert_eq!(avatar_filter(&c, &base["id"]), members);
    c.delete("product", "900202").unwrap();
    // A second avatar with that name leaves the label's owner unknown.
    synthetic_named_base(&c, "900203", "オリジナル3Dモデル「ソラネコ -Skycat-」");
    let check_apart = |c: &Catalogue, extra: &[&'static str]| {
        let derived = avatar_rows(c, "avatar:");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0]["id"], id);
        assert_eq!(derived[0]["displayName"], "Saved name");
        let mut members = vec!["manual", "names-base"];
        members.extend_from_slice(extra);
        members.sort();
        assert_eq!(avatar_filter(c, &id), members);
    };
    check_apart(&c, &[]);
    c.delete("product", "900203").unwrap();
    assert!(avatar_rows(&c, "avatar:").is_empty());
    // So does a link to another product under that name.
    citing_product(&c, "cites-other", "900204", "ソラネコ");
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    check_apart(&c, &["cites-other"]);
    assert_eq!(avatar_filter(&c, &base["id"]), ["900201"]);
    c.delete("remote_file", "cites-other-f").unwrap();
    c.delete("entitlement", "cites-other-e").unwrap();
    c.delete("product", "cites-other").unwrap();

    c.delete("product", "900201").unwrap();
    let row = avatar_rows(&c, "avatar:")[0].clone();
    assert_eq!(row["id"], id);
    assert_eq!(row["displayName"], "Saved name");
    assert_eq!(row["matchingProductCount"], 2);
    assert_eq!(avatar_filter(&c, &id), ["manual", "names-base"]);
}

#[test]
fn bilingual_and_decorated_labels_preserve_source_and_product_identity() {
    let f = Fixture::new();
    let c = f.open();
    let title = "オリジナル3Dモデル「ソラネコ / Soraneko」Ver.1.2";
    for (id, source, shop) in [
        ("pair-a", title, "Authored A"),
        ("pair-b", title, "Authored B"),
        ("decorated", "★シロネコ★", "Authored C"),
    ] {
        c.put("product",id,json!({"id":id,"sourceTitle":source,"sourceCategory":{"name":"3Dアバター"},"shop":shop})).unwrap();
    }
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let options = boot["avatarOptions"].as_array().unwrap();
    for id in ["pair-a", "pair-b"] {
        let a = options
            .iter()
            .find(|a| text(a, "id") == format!("product:{id}"))
            .unwrap();
        assert_eq!(a["displayName"], "ソラネコ / Soraneko");
        assert_eq!(a["englishNameStatus"], "sourceTitleSpelling");
        assert_eq!(a["baseProductIds"], json!([id]));
        assert_eq!(c.get("product", id).unwrap().unwrap()["sourceTitle"], title);
    }
    let decorated = options
        .iter()
        .find(|a| a["id"] == "product:decorated")
        .unwrap();
    assert_eq!(decorated["englishName"], "Shironeko");
    assert_eq!(text(decorated, "displayName").matches(" / ").count(), 1);
}

#[test]
fn product_nicknames_never_rename_canonical_avatar_labels() {
    let f = Fixture::new();
    let c = f.open();
    for (id, title) in [
        ("5007531", "真冬"),
        ("synthetic-base", "シロネコ"),
        ("linked-base", "Second edition"),
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"sourceTitle":title,"sourceCategory":{"name":"3Dアバター"}}),
        )
        .unwrap();
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"title":format!("Nickname {id}")}}),
        )
        .unwrap();
    }
    c.command(
        "overlay",
        json!({"ids":["linked-base"],"patch":{"ownedAvatars":["Mafuyu"]}}),
    )
    .unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let entries = boot["avatarOptions"].as_array().unwrap();
    let known = entries.iter().find(|a| a["id"] == "booth:5007531").unwrap();
    assert_eq!(known["displayName"], "真冬 / Mafuyu");
    assert!(strings(&known["aliases"]).contains(&"Nickname linked-base".to_owned()));
    assert_eq!(
        entries
            .iter()
            .find(|a| a["id"] == "product:synthetic-base")
            .unwrap()["displayName"],
        "シロネコ / Shironeko"
    );
    assert_eq!(
        c.command("product_details", json!({"id":"5007531"}))
            .unwrap()["title"],
        "Nickname 5007531"
    );
    c.command("preferences",json!({"patch":{"avatarSettings":{"booth:5007531":{"originalName":"Human identity","englishName":"Human English"}}}})).unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(
        boot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "booth:5007531")
            .unwrap()["displayName"],
        "Human identity / Human English"
    );
}

#[test]
fn explicit_manual_base_identity_unions_pins_without_same_name_merges() {
    let f = Fixture::new();
    let c = f.open();
    for (id, name, shop) in [
        ("5007531", "真冬", "Official"),
        ("manual-base", "Authored edition", "Other"),
        ("ambiguous-base", "真冬", "Unrelated"),
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"sourceTitle":name,"shop":shop,"sourceCategory":{"name":"3Dアバター"}}),
        )
        .unwrap();
    }
    c.command(
        "overlay",
        json!({"ids":["manual-base"],"patch":{"ownedAvatars":["Mafuyu"]}}),
    )
    .unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let options = boot["avatarOptions"].as_array().unwrap();
    let known = options.iter().find(|a| a["id"] == "booth:5007531").unwrap();
    assert_eq!(known["baseProductIds"], json!(["5007531", "manual-base"]));
    assert_eq!(known["matchingProductCount"], 2);
    assert!(!options.iter().any(|a| a["id"] == "product:manual-base"));
    assert!(options.iter().any(|a| a["id"] == "product:ambiguous-base"));
    let query=c.command("query",json!({"avatar":"Mafuyu","category":"Hair","search":"no match","accountIds":[],"limit":1,"offset":1})).unwrap();
    assert_eq!(query["matchedIds"], json!(["5007531", "manual-base"]));
    assert_eq!(query["products"][0]["id"], "manual-base");
    let without_offline=c.command("query",json!({"avatar":"Mafuyu","category":"Hair","search":"no match","accountIds":[],"includeOffline":false})).unwrap();
    assert_eq!(without_offline["matchedIds"], query["matchedIds"]);
}

#[test]
fn canonical_avatar_registry_counts_aliases_and_preserves_manual_disable() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","6538026",json!({"id":"6538026","sourceTitle":"オリジナル3Dモデル『ミルティナ』","shop":"DOLOS art","sourceCategory":{"name":"3Dアバター"},"sourceTags":["VRChat"]})).unwrap();
    for n in 0..6 {
        let id = format!("compat{n}");
        product(&c, &id);
        c.command("overlay",json!({"ids":[id],"patch":{"ownedAvatars":["ミルティナ","Milltina","オリジナル3Dモデル『ミルティナ』"]}})).unwrap();
    }
    c.put("preferences","app",json!({"id":"app","avatarSettings":{"booth:6538026":{"enabled":false,"originalName":"Human name","englishName":"Human English","aliases":["Human alias"]}}})).unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let entries = boot["avatarOptions"].as_array().unwrap();
    let a = entries.iter().find(|v| v["id"] == "booth:6538026").unwrap();
    assert_eq!(a["matchingProductCount"], 7);
    assert_eq!(a["enabled"], false);
    assert_eq!(a["displayName"], "Human name / Human English");
    assert_eq!(a["baseProductIds"], json!(["6538026"]));
    assert_eq!(
        entries
            .iter()
            .filter(|v| v["productId"] == "6538026")
            .count(),
        1
    );
    c.command(
        "overlay",
        json!({"ids":["compat0"],"patch":{"ownedAvatars":["Human name / Human English"]}}),
    )
    .unwrap();
    assert_eq!(
        c.command("query", json!({"avatar":"Human name / Human English"}))
            .unwrap()["matched"],
        7
    );
    let refreshed = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(
        refreshed["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "booth:6538026")
            .unwrap()["matchingProductCount"],
        7
    );
    assert!(!refreshed["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| text(a, "id").starts_with("avatar:")));
    let revision = c.revision().unwrap();
    c.command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(
        revision,
        c.revision().unwrap(),
        "bootstrap reads must not change revisions"
    );
    drop(c);
    let c = f.open();
    assert_eq!(
        c.command("query", json!({"avatar":"Human alias"})).unwrap()["matched"],
        7
    );
    let prefs = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(prefs["avatarSettings"]["booth:6538026"]["enabled"], false);
}

#[test]
fn avatar_base_pins_override_ordinary_filters_before_paging_without_duplicates() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","5007531",json!({"id":"5007531","sourceTitle":"真冬","sourceCategory":{"name":"3Dアバター"},"sensitive":true})).unwrap();
    for id in ["one", "two"] {
        product(&c, id);
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"ownedAvatars":["Mafuyu"]}}),
        )
        .unwrap();
    }
    let hidden=c.command("query",json!({"avatar":"booth:5007531","category":"Clothes","collectionId":"missing","search":"no match","quick":"downloaded","sensitive":"show","accountIds":[],"sort":"price","limit":1})).unwrap();
    assert_eq!(hidden["matchedIds"], json!(["5007531"]));
    assert_eq!(hidden["matched"], 1);
    let first = c
        .command(
            "query",
            json!({"avatar":"Mafuyu","category":"Hair","sort":"title","limit":1}),
        )
        .unwrap();
    assert_eq!(first["matched"], 3);
    assert_eq!(first["products"][0]["id"], "5007531");
    let rest = c
        .command(
            "query",
            json!({"avatar":"Mafuyu","category":"Hair","sort":"title","offset":1,"limit":9}),
        )
        .unwrap();
    assert_eq!(rest["products"].as_array().unwrap().len(), 2);
    assert!(rest["products"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["id"] != "5007531"));
    c.command(
        "overlay",
        json!({"ids":["5007531"],"patch":{"ownedAvatars":["Mafuyu"]}}),
    )
    .unwrap();
    assert_eq!(
        c.command("query", json!({"avatar":"Mafuyu"})).unwrap()["matched"],
        3
    );
}

#[test]
fn avatar_base_pins_respect_r18_hide_only_and_hidden_boundaries() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product", "5007531", json!({"id":"5007531","provider":"booth","sourceTitle":"真冬","sourceCategory":{"name":"3Dアバター"},"sensitive":true})).unwrap();
    let query = |mode| {
        c.command("query", json!({"avatar":"booth:5007531","sensitive":mode}))
            .unwrap()["matchedIds"]
            .clone()
    };
    assert_eq!(query("hide"), json!([]));
    assert_eq!(query("only"), json!(["5007531"]));
    c.command(
        "overlay",
        json!({"ids":["5007531"],"patch":{"sensitive":false}}),
    )
    .unwrap();
    assert_eq!(query("only"), json!([]));
    assert_eq!(query("hide"), json!(["5007531"]));
    c.command(
        "overlay",
        json!({"ids":["5007531"],"patch":{"hidden":true}}),
    )
    .unwrap();
    assert_eq!(query("show"), json!([]));
    assert_eq!(
        c.command(
            "query",
            json!({"avatar":"booth:5007531","sensitive":"hide","hiddenOnly":true})
        )
        .unwrap()["matchedIds"],
        json!(["5007531"])
    );
}

#[test]
fn avatar_default_threshold_and_unknown_base_identity_are_independent() {
    let f = Fixture::new();
    let c = f.open();
    for n in 0..5 {
        let id = format!("member{n}");
        product(&c, &id);
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"ownedAvatars":["Maya"]}}),
        )
        .unwrap();
    }
    c.put("product","synthetic-base",json!({"id":"synthetic-base","sourceTitle":"真冬","shop":"Independent author","sourceCategory":{"name":"3Dアバター"}})).unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    let entries = boot["avatarOptions"].as_array().unwrap();
    assert_eq!(
        entries.iter().find(|v| v["id"] == "booth:3390957").unwrap()["enabled"],
        false
    );
    assert_eq!(
        entries
            .iter()
            .find(|v| v["id"] == "product:synthetic-base")
            .unwrap()["baseProductIds"],
        json!(["synthetic-base"])
    );
    assert_eq!(
        c.command("query", json!({"avatar":"booth:5007531"}))
            .unwrap()["matched"],
        0
    );
    product(&c, "member5");
    c.command(
        "overlay",
        json!({"ids":["member5"],"patch":{"ownedAvatars":["Maya"]}}),
    )
    .unwrap();
    let maya = |c: &Catalogue| {
        c.command("bootstrap", json!({"includeProducts":false}))
            .unwrap()["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == "booth:3390957")
            .unwrap()
            .clone()
    };
    assert_eq!(maya(&c)["matchingProductCount"], 6);
    assert_eq!(
        maya(&c)["enabled"],
        false,
        "Compatible clothes do not establish Base ownership"
    );
    c.put(
        "product",
        "3390957",
        json!({"id":"3390957","sourceTitle":"舞夜","sourceCategory":{"name":"3Dアバター"}}),
    )
    .unwrap();
    assert_eq!(
        maya(&c)["enabled"],
        false,
        "Ingestion does not toggle the avatar mid-sync"
    );
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(maya(&c)["enabled"], true);
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"booth:3390957":{"enabled":false}}}}),
    )
    .unwrap();
    assert_eq!(
        maya(&c)["enabled"],
        false,
        "Manual off wins over count and owned Base"
    );
    c.delete("product", "3390957").unwrap();
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"booth:3390957":{"enabled":true}},"uiScale":999}}),
    )
    .unwrap();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert_eq!(boot["preferences"]["uiScale"], 300);
    assert_eq!(
        boot["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == "booth:3390957")
            .unwrap()["enabled"],
        true
    );
}

#[test]
fn avatar_automatic_defaults_change_only_at_sync_closure_and_survive_reopen() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let enabled = |catalogue: &Catalogue| {
        catalogue
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap()["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "booth:3390957")
            .unwrap()["enabled"]
            .clone()
    };
    product(&c, "first-clothes");
    c.command(
        "overlay",
        json!({"ids":["first-clothes"],"patch":{"ownedAvatars":["Maya"]}}),
    )
    .unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(enabled(&c), false);
    c.put(
        "job",
        "sync",
        json!({"id":"sync","type":"boothSync","mode":"incremental","state":"running"}),
    )
    .unwrap();
    for n in 0..6 {
        let id = format!("later-clothes-{n}");
        product(&c, &id);
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"ownedAvatars":["Maya"]}}),
        )
        .unwrap();
        assert_eq!(
            enabled(&c),
            false,
            "A bootstrap during sync only reads the previous default"
        );
    }
    c.put(
        "product",
        "3390957",
        json!({"id":"3390957","sourceTitle":"舞夜","sourceCategory":{"name":"3Dアバター"}}),
    )
    .unwrap();
    assert_eq!(
        enabled(&c),
        false,
        "A late Base does not change the default during sync"
    );
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        enabled(&c),
        false,
        "An active sync prevents automatic analysis"
    );
    c.put(
        "job",
        "sync-other-store",
        json!({"id":"sync-other-store","type":"gumroadSync","state":"queued"}),
    )
    .unwrap();
    c.put(
        "job",
        "sync",
        json!({"id":"sync","type":"boothSync","mode":"incremental","state":"completed"}),
    )
    .unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        enabled(&c),
        false,
        "Another provider's queued sync retains defaults"
    );
    c.put(
        "job",
        "sync-other-store",
        json!({"id":"sync-other-store","type":"gumroadSync","state":"cancelled"}),
    )
    .unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(enabled(&c), true);
    assert_eq!(
        c.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"]["booth:3390957"],
        true
    );
    drop(c);
    let c = fixture.open();
    assert_eq!(
        enabled(&c),
        true,
        "Saved automatic defaults survive restart"
    );
    c.put(
        "job",
        "sync-full",
        json!({"id":"sync-full","type":"boothSync","mode":"forced","state":"running"}),
    )
    .unwrap();
    c.delete("product", "3390957").unwrap();
    assert_eq!(
        enabled(&c),
        true,
        "A full refresh retains the prior default until closure"
    );
    c.put(
        "job",
        "sync-full",
        json!({"id":"sync-full","type":"boothSync","mode":"forced","state":"failed"}),
    )
    .unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        enabled(&c),
        false,
        "Terminal partial failures recompute from currently available records"
    );
    let revision = c.revision().unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        revision,
        c.revision().unwrap(),
        "Unchanged recomputation is idempotent"
    );
}

#[test]
fn avatar_user_overrides_remain_separate_from_recomputed_defaults() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "product",
        "3390957",
        json!({"id":"3390957","sourceTitle":"舞夜","sourceCategory":{"name":"3Dアバター"}}),
    )
    .unwrap();
    for n in 0..6 {
        let id = format!("compatible-{n}");
        product(&c, &id);
        c.command(
            "overlay",
            json!({"ids":[id],"patch":{"ownedAvatars":["Maya"]}}),
        )
        .unwrap();
    }
    let enabled = |catalogue: &Catalogue| {
        catalogue
            .command("bootstrap", json!({"includeProducts":false}))
            .unwrap()["avatarOptions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "booth:3390957")
            .unwrap()["enabled"]
            .clone()
    };
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"booth:3390957":{"enabled":false}}}}),
    )
    .unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        enabled(&c),
        false,
        "Manual off wins over the automatic enabled snapshot"
    );
    let prefs = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(prefs["avatarSettings"]["booth:3390957"]["enabled"], false);
    assert_eq!(prefs["avatarAutomaticEnabled"]["booth:3390957"], true);
    c.command(
        "preferences",
        json!({"patch":{"avatarSettings":{"booth:3390957":{"enabled":true}}}}),
    )
    .unwrap();
    c.delete("product", "3390957").unwrap();
    c.refresh_avatar_defaults().unwrap();
    assert_eq!(
        enabled(&c),
        true,
        "Manual on survives an automatic off result"
    );
    drop(c);
    let c = fixture.open();
    assert_eq!(enabled(&c), true, "Manual on survives restart");
    assert_eq!(
        c.get("preferences", "app").unwrap().unwrap()["avatarAutomaticEnabled"]["booth:3390957"],
        Value::Null
    );
}
