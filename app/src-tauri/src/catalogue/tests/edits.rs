//! User edits: undo, tags, categories, collections and overlays.
use super::*;

#[test]
fn undo_history_is_bounded_and_noop_edits_do_not_write_records_or_revisions() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "authored-undo");
    c.inner.lock().unwrap().execute_batch("WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value+1 FROM n WHERE value<100) INSERT INTO undo(token,body) SELECT CAST(value AS TEXT),'[]' FROM n").unwrap();
    let changed = c
        .command(
            "overlay",
            json!({"ids":["authored-undo"],"patch":{"note":"Saved note"}}),
        )
        .unwrap();
    assert_eq!(changed["changed"], 1);
    let revision = c.revision().unwrap();
    let unchanged = c
        .command(
            "overlay",
            json!({"ids":["authored-undo"],"patch":{"note":"Saved note"}}),
        )
        .unwrap();
    assert_eq!(unchanged["changed"], 0);
    assert!(unchanged.get("undoToken").is_none());
    assert_eq!(c.revision().unwrap(), revision);
    assert_eq!(
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM undo", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        100
    );
    assert!(
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM undo WHERE token='1'", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap()
            == 0
    );
    c.command("undo", json!({"token":changed["undoToken"]}))
        .unwrap();
}

#[test]
fn tag_edits_reject_only_visible_tags_and_preserve_new_hidden_source_compatibility() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let mut source = json!({"id":"authored-tag-edit","title":"Authored clothes","sourceCategory":{"name":"Clothes"},"sourceTags":["Cute"]});
    c.put("product", "authored-tag-edit", source.clone())
        .unwrap();
    c.command(
        "overlay",
        json!({"ids":["authored-tag-edit"],"patch":{"tags":["Cute","Mine"]}}),
    )
    .unwrap();
    source["sourceTags"] = json!(["Cute", "Mafuyu"]);
    c.put("product", "authored-tag-edit", source).unwrap();
    let details = || {
        c.command("product_details", json!({"id":"authored-tag-edit"}))
            .unwrap()
    };
    assert_eq!(details()["tags"], json!(["Cute", "Mine"]));
    assert!(details()["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["avatarId"] == "booth:5007531"));
    c.command(
        "overlay",
        json!({"ids":["authored-tag-edit"],"patch":{"tags":["Cute","Mine","Fav"]}}),
    )
    .unwrap();
    assert!(
        !strings(&c.get("overlay", "authored-tag-edit").unwrap().unwrap()["rejectedTags"])
            .contains(&"Mafuyu".into())
    );
    assert_eq!(
        c.command("query", json!({"avatar":"booth:5007531"}))
            .unwrap()["matchedIds"],
        json!(["authored-tag-edit"])
    );
    c.command(
        "overlay",
        json!({"ids":["authored-tag-edit"],"patch":{},"clearFields":["tags"]}),
    )
    .unwrap();
    assert!(strings(&details()["tags"]).contains(&"Mafuyu".into()));
    // A tag actually removed from the displayed list still remains rejected.
    c.command(
        "overlay",
        json!({"ids":["authored-tag-edit"],"patch":{"tags":["Cute"]}}),
    )
    .unwrap();
    assert!(
        strings(&c.get("overlay", "authored-tag-edit").unwrap().unwrap()["rejectedTags"])
            .contains(&"Mafuyu".into())
    );
}

#[test]
fn collection_results_are_atomic_and_revisions_preserve_broad_changes() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "synthetic-member");
    let before = c.change_revisions().unwrap();
    assert_eq!(before.0, before.1);
    // Exercise upgrade fallback, including the first collection-only mutation.
    c.inner
        .lock()
        .unwrap()
        .execute("DELETE FROM meta WHERE key='noncollection_revision'", [])
        .unwrap();
    assert_eq!(c.change_revisions().unwrap(), before);
    let added = c.command("collection", json!({"op":"create","id":"synthetic-list","name":"Synthetic list","productIds":["synthetic-member"]})).unwrap();
    assert_eq!(
        added["collections"][0]["productIds"],
        json!(["synthetic-member"])
    );
    assert!(c
        .get("collection", "synthetic-list")
        .unwrap()
        .unwrap()
        .get("collections")
        .is_none());
    let after = c.change_revisions().unwrap();
    assert!(after.0 > before.0);
    assert_eq!(after.1, before.0);
    let dedup = c
        .command(
            "collection",
            json!({"op":"add","collectionIds":["synthetic-list"],"ids":["synthetic-member"]}),
        )
        .unwrap();
    assert_eq!(dedup["collections"], added["collections"]);
    assert_eq!(c.change_revisions().unwrap(), after);
    product(&c, "synthetic-second");
    let source = c.change_revisions().unwrap();
    assert!(source.0 > after.0);
    assert_eq!(source.0, source.1);
    let removed = c
        .command("collection", json!({"op":"delete","id":"synthetic-list"}))
        .unwrap();
    assert_eq!(removed["collections"], json!([]));
    let deleted = c.change_revisions().unwrap();
    assert!(deleted.0 > source.0);
    assert_eq!(deleted.1, source.1);
    drop(c);
    assert_eq!(f.open().change_revisions().unwrap(), deleted);
}

#[test]
fn lightweight_collections_reads_memberships_without_normalizing_avatar_preferences() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "synthetic-member");
    let before = c.get("preferences", "app").unwrap();
    c.command("collection", json!({"operation":"add","names":["Synthetic list","Synthetic list"],"ids":["synthetic-member"]})).unwrap();
    let revision = c.revision().unwrap();
    let result = c.command("collections", json!({})).unwrap();
    let rows = result["collections"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["productIds"], json!(["synthetic-member"]));
    assert_eq!(c.revision().unwrap(), revision);
    assert_eq!(c.get("preferences", "app").unwrap(), before);
}

#[test]
fn opening_after_a_name_rule_change_resolves_stored_tags_once() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product", "base", json!({"id":"base","sourceTitle":"【販売終了】ソラネコ -Soraneko- / オリジナル3Dモデル","sourceCategory":{"name":"3Dアバター"}})).unwrap();
    c.put("product", "coat", json!({"id":"coat","sourceTitle":"Authored coat","sourceCategory":{"name":"3D衣装"},"sourceTags":["Soraneko"]})).unwrap();
    let resolved = BTreeSet::from(["product:base".to_owned()]);
    assert_eq!(dynamic_tag_ids(&c, "coat"), resolved);
    // A library written by an older build holds that build's resolution.
    c.inner.lock().unwrap().execute_batch("UPDATE products SET body=json_set(body,'$.ownedAvatarEvidence',json('[]'),'$.ownedAvatars',json('[]')),summary=json_set(summary,'$.ownedAvatarEvidence',json('[]'),'$.ownedAvatars',json('[]')) WHERE id='coat'; UPDATE meta SET value='older' WHERE key='taxonomy_version';").unwrap();
    assert!(dynamic_tag_ids(&c, "coat").is_empty());
    drop(c);
    let c = f.open();
    assert_eq!(dynamic_tag_ids(&c, "coat"), resolved);
    let revision = c.revision().unwrap();
    drop(c);
    assert_eq!(f.open().revision().unwrap(), revision);
}

#[test]
fn a_category_rule_reaches_a_stored_product_only_through_its_own_records() {
    let f = Fixture::new();
    let c = f.open();
    let source = |id: &str, tags: Value| json!({"id":id,"sourceTitle":"オリジナル3Dモデル「ソラネコ」","sourceCategory":{"name":"3Dキャラクター"},"sourceTags":tags});
    let category = |c: &Catalogue, id: &str| -> String {
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT category FROM products WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    };
    c.put("product", "stored", source("stored", json!(["髪型"])))
        .unwrap();
    c.put("product", "manual", source("manual", json!(["髪型"])))
        .unwrap();
    c.command(
        "overlay",
        json!({"ids":["manual"],"patch":{"category":"Tool"}}),
    )
    .unwrap();
    assert_eq!(category(&c, "stored"), "Base");
    assert_eq!(category(&c, "manual"), "Tool");
    // A library written by an older build holds that build's category.
    c.inner.lock().unwrap().execute_batch("UPDATE products SET category='Hair',body=json_set(body,'$.category','Hair'),summary=json_set(summary,'$.category','Hair') WHERE id='stored'; UPDATE meta SET value='older' WHERE key='taxonomy_version';").unwrap();
    drop(c);
    let c = f.open();
    assert_eq!(category(&c, "stored"), "Hair");
    assert_eq!(category(&c, "manual"), "Tool");
    // A new observation applies the current rules, and a manual category stays.
    c.put(
        "product",
        "stored",
        source("stored", json!(["髪型", "VRChat"])),
    )
    .unwrap();
    c.put(
        "product",
        "manual",
        source("manual", json!(["髪型", "VRChat"])),
    )
    .unwrap();
    assert_eq!(category(&c, "stored"), "Base");
    assert_eq!(category(&c, "manual"), "Tool");
}

#[test]
fn a_manual_category_outlasts_the_store_category_rule() {
    let f = Fixture::new();
    let c = f.open();
    let source = |tags: Value| json!({"id":"room","store":"BOOTH","sourceTitle":"【販売ワールド】Authored Room","sourceCategory":{"name":"3D環境・ワールド","parentName":"3Dモデル"},"sourceTags":tags});
    let category = |c: &Catalogue| -> String {
        c.inner
            .lock()
            .unwrap()
            .query_row("SELECT category FROM products WHERE id='room'", [], |r| {
                r.get(0)
            })
            .unwrap()
    };
    c.put("product", "room", source(json!(["VRChat", "Udon"])))
        .unwrap();
    assert_eq!(category(&c), "World");
    c.command(
        "overlay",
        json!({"ids":["room"],"patch":{"category":"Udon"}}),
    )
    .unwrap();
    assert_eq!(category(&c), "Udon");
    // A new observation runs the rule again and the user's choice stays.
    c.put(
        "product",
        "room",
        source(json!(["VRChat", "Udon", "UdonSharp"])),
    )
    .unwrap();
    assert_eq!(category(&c), "Udon");
    drop(c);
    let c = f.open();
    assert_eq!(category(&c), "Udon");
    // Clearing it hands the product back to the rule.
    c.command(
        "overlay",
        json!({"ids":["room"],"patch":{},"clearFields":["category"]}),
    )
    .unwrap();
    assert_eq!(category(&c), "World");
}

#[test]
fn explicit_public_404_evidence_hides_source_link_without_changing_raw_or_custom_url() {
    let f = Fixture::new();
    let c = f.open();
    let paired = json!({"id":"current-404","store":"BOOTH","sourceTitle":"Synthetic retained source","sourceUrl":"https://booth.pm/ja/items/999999","publicPageAvailability":"unavailable"});
    c.put("product", "current-404", paired.clone()).unwrap();
    assert_eq!(projection(&c, "current-404")["sourceUrlAvailable"], false);
    assert_eq!(
        projection(&c, "current-404")["publicPageAvailability"],
        "unavailable"
    );
    assert_eq!(
        c.get("product", "current-404").unwrap(),
        Some(paired.clone())
    );
    for (id, patch, available) in [
        ("unknown", json!({"publicPageAvailability":"unknown"}), true),
        (
            "available",
            json!({"publicPageAvailability":"available"}),
            true,
        ),
        (
            "unavailable",
            json!({"publicPageAvailability":"unavailable"}),
            false,
        ),
        (
            "other-store",
            json!({"store":"Gumroad","publicPageAvailability":"unavailable"}),
            false,
        ),
    ] {
        let mut source = paired.clone();
        source["id"] = json!(id);
        for (k, v) in patch.as_object().unwrap() {
            source[k] = v.clone();
        }
        c.put("product", id, source).unwrap();
        assert_eq!(projection(&c, id)["sourceUrlAvailable"], available, "{id}");
    }
    c.command(
        "overlay",
        json!({"ids":["current-404"],"patch":{"sourceUrl":"https://example.org/replacement"}}),
    )
    .unwrap();
    assert_eq!(projection(&c, "current-404")["sourceUrlAvailable"], true);
    c.command(
        "overlay",
        json!({"ids":["current-404"],"patch":{"sourceUrl":""}}),
    )
    .unwrap();
    assert_eq!(projection(&c, "current-404")["sourceUrlAvailable"], false);
    c.command(
        "overlay",
        json!({"ids":["current-404"],"patch":{},"clearFields":["sourceUrl"]}),
    )
    .unwrap();
    let reset = projection(&c, "current-404");
    assert_eq!(reset["sourceUrl"], paired["sourceUrl"]);
    assert_eq!(reset["sourceUrlAvailable"], false);
    assert_eq!(reset["sourceUrlOverride"], false);
    assert_eq!(
        c.get("product", "current-404").unwrap(),
        Some(paired.clone())
    );
    drop(c);
    let c = f.open();
    assert_eq!(projection(&c, "current-404")["sourceUrlAvailable"], false);
    assert_eq!(c.get("product", "current-404").unwrap(), Some(paired));
}

#[test]
fn source_links_provenance_and_unclassified_vr_scope_preserve_manual_choices() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","synthetic",json!({"id":"synthetic","sourceTitle":"Uncategorized authored product","store":"BOOTH","sourceUrl":"https://booth.pm/items/999999","publicPageAvailability":"unavailable","sourceTags":["VRChat"]})).unwrap();
    for id in ["a", "b"] {
        c.put(
            "account",
            id,
            json!({"id":id,"store":"BOOTH","name":format!("Account {id}")}),
        )
        .unwrap();
        let aid = format!("acq-{id}");
        c.put(
            "acquisition",
            &aid,
            json!({"id":aid,"productId":"synthetic","accountId":id}),
        )
        .unwrap();
    }
    let p = projection(&c, "synthetic");
    assert_eq!(p["category"], "NonVRChat");
    assert_eq!(p["sourceUrlAvailable"], false);
    assert_eq!(p["sources"].as_array().unwrap().len(), 2);
    assert_eq!(p["sources"][0]["label"], "BOOTH – Account a");
    assert_eq!(c.command("query", json!({})).unwrap()["matched"], 1);
    c.command("overlay",json!({"ids":["synthetic"],"patch":{"sourceUrl":"https://example.org/item?view=full#about","category":"NonVRChat"}})).unwrap();
    assert_eq!(projection(&c, "synthetic")["sourceUrlAvailable"], true);
    assert_eq!(
        c.get("product", "synthetic").unwrap().unwrap()["sourceUrl"],
        "https://booth.pm/items/999999"
    );
    assert_eq!(c.command("query", json!({})).unwrap()["matched"], 0);
    assert_eq!(
        c.command("query", json!({"category":"NonVRChat"})).unwrap()["matched"],
        1
    );
    assert!(c
        .command(
            "overlay",
            json!({"ids":["synthetic"],"patch":{"sourceUrl":"https://user:secret@example.org/"}})
        )
        .is_err());
    c.command(
        "overlay",
        json!({"ids":["synthetic"],"patch":{"sourceUrl":""}}),
    )
    .unwrap();
    assert_eq!(projection(&c, "synthetic")["sourceUrlAvailable"], false);
}

#[test]
fn gumroad_scope_rule_applies_to_new_source_sync_without_reclassifying_on_open() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let id = "gumroad:authored-existing-shader";
    let mut source = json!({"id":id,"provider":"gumroad","store":"Gumroad",
        "sourceTitle":"Authored shader for Blender","sourceTags":["VRChat","Shader"],
        "observedAt":"2026-10-01T00:00:00Z"});
    c.put("product", id, source.clone()).unwrap();
    // Preserve a category materialized before the stricter Gumroad rule.
    let old = json!({"category":"Shader","basis":"tags","inferred":true,
        "evidence":[],"scope":"vrchat","scopeEvidence":{"vrchat":true}});
    c.inner.lock().unwrap().execute(
        "UPDATE products SET category='Shader',in_all=1,body=json_set(body,'$.category','Shader','$.categoryInference',json(?2)),summary=json_set(summary,'$.category','Shader','$.categoryInference',json(?2)) WHERE id=?1",
        params![id,serde_json::to_string(&old).unwrap()],
    ).unwrap();
    drop(c);

    let reopened = fixture.open();
    assert_eq!(projection(&reopened, id)["category"], "Shader");
    assert_eq!(reopened.get("product", id).unwrap().unwrap(), source);
    reopened.put("product", id, source.clone()).unwrap();
    assert_eq!(projection(&reopened, id)["category"], "Shader");

    source["observedAt"] = json!("2026-10-14T00:00:00Z");
    reopened.put("product", id, source).unwrap();
    assert_eq!(projection(&reopened, id)["category"], "NonVRChat");
    assert_eq!(reopened.command("query", json!({})).unwrap()["matched"], 0);
    assert_eq!(
        reopened
            .command("query", json!({"category":"NonVRChat"}))
            .unwrap()["matchedIds"],
        json!([id])
    );
}

#[test]
fn atomic_batch_and_collections() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    product(&c, "2");
    assert!(c
        .command(
            "overlay",
            json!({"ids":["1","missing"],"patch":{"note":"must rollback"}})
        )
        .is_err());
    assert_eq!(projection(&c, "1")["note"], "");
    c.command("collection", json!({"op":"create","id":"a","name":"A"}))
        .unwrap();
    c.command("collection", json!({"op":"create","id":"b","name":"B"}))
        .unwrap();
    c.command(
        "collection",
        json!({"op":"add","collectionIds":["a","b"],"ids":["1","2"]}),
    )
    .unwrap();
    assert_eq!(
        c.get("collection", "a").unwrap().unwrap()["productIds"],
        json!(["1", "2"])
    );
    assert!(c
        .command(
            "collection",
            json!({"op":"add","collectionIds":["a","missing"],"ids":["1"]})
        )
        .is_err());
}

#[test]
fn exact_tag_equivalence_and_manual_tags_survive() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","1",json!({"id":"1","title":"Synthetic","tags":["Casual","Mafuyu coat","Mafuyu"],"sourceCategory":{"name":"Hair"}})).unwrap();
    c.command(
        "overlay",
        json!({"ids":["1"],"patch":{"tags":["カジュアル","Mafuyu"],"ownedAvatars":["真冬"]}}),
    )
    .unwrap();
    let p = projection(&c, "1");
    assert!(strings(&p["tags"]).contains(&"カジュアル".into()));
    assert!(strings(&p["tags"]).contains(&"Mafuyu".into()));
    assert!(!strings(&p["rejectedTags"]).contains(&"Casual".into()));
    assert_eq!(
        c.command("query", json!({"avatar":"booth:5007531"}))
            .unwrap()["matchedIds"],
        json!(["1"])
    );
    c.command(
        "overlay",
        json!({"ids":["1"],"patch":{"tags":["Mafuyu"],"ownedAvatars":[]}}),
    )
    .unwrap();
    assert!(strings(&projection(&c, "1")["tags"]).contains(&"Mafuyu".into()));
}

#[test]
fn heterogeneous_overlay_is_atomic_and_undo_field_aware() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    product(&c, "2");
    let result=c.command("overlay",json!({"changes":[{"id":"1","patch":{"title":"One"}},{"id":"2","patch":{"note":"Two"}}]})).unwrap();
    c.command("overlay", json!({"ids":["1"],"patch":{"note":"Unrelated"}}))
        .unwrap();
    let undone = c
        .command("undo", json!({"token":result["undoToken"]}))
        .unwrap();
    assert_eq!(undone["changed"], 2);
    assert_eq!(projection(&c, "1")["note"], "Unrelated");
}

#[test]
fn custom_categories_stay_manual_and_deleting_one_restores_automatic_classification() {
    let fixture = Fixture::new();
    let c = fixture.open();
    // Backups made before custom categories have no record list for them.
    let mut old = c.manifest().unwrap();
    old["records"]
        .as_object_mut()
        .unwrap()
        .remove("custom_category");
    let restored = Fixture::new();
    restored.open().restore_manifest(old).unwrap();
    product(&c, "hair");
    c.put(
        "product",
        "plain",
        json!({"id":"plain","title":"Authored unrelated print"}),
    )
    .unwrap();
    let create = |name: &str| c.command("custom_category", json!({"op":"create","name":name}));
    let props = text(&create("Props").unwrap()["customCategories"][0], "id").to_owned();
    assert!(props.starts_with("custom:"));
    assert!(create(" ｐｒｏｐｓ ").is_err());
    for missing in [
        format!("custom:{}", Uuid::new_v4()),
        "custom:authored".into(),
    ] {
        assert!(c
            .command(
                "overlay",
                json!({"ids":["hair"],"patch":{"category":missing}})
            )
            .is_err());
    }
    let category = |id| projection(&c, id)["category"].clone();
    let in_all = |id| {
        c.command("query", json!({})).unwrap()["matchedIds"]
            .as_array()
            .unwrap()
            .contains(&json!(id))
    };
    assert!(!in_all("plain"));
    c.command(
        "overlay",
        json!({"ids":["hair","plain"],"patch":{"category":props}}),
    )
    .unwrap();
    assert_eq!(
        c.command("query", json!({"category":props})).unwrap()["matchedIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(in_all("plain"));
    // A new source observation cannot reclassify a manually chosen custom category.
    c.put(
        "product",
        "hair",
        json!({"id":"hair","title":"Authored shader","sourceCategory":{"name":"Shader"}}),
    )
    .unwrap();
    assert_eq!(category("hair"), props);
    c.command(
        "custom_category",
        json!({"op":"update","id":props,"name":"Small props"}),
    )
    .unwrap();
    create("Kept").unwrap();
    let away = c
        .command(
            "overlay",
            json!({"ids":["hair"],"patch":{"category":"Tool"}}),
        )
        .unwrap();
    let copy = Fixture::new();
    let peer = copy.open();
    peer.restore_manifest(c.manifest().unwrap()).unwrap();
    assert_eq!(projection(&peer, "plain")["category"], props);
    assert_eq!(
        peer.command("bootstrap", json!({})).unwrap()["customCategories"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let remaining = c
        .command("custom_category", json!({"op":"delete","id":props}))
        .unwrap();
    assert_eq!(
        remaining["customCategories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| text(row, "name"))
            .collect::<Vec<_>>(),
        ["Kept"]
    );
    assert!(c
        .get("overlay", "plain")
        .unwrap()
        .unwrap()
        .get("category")
        .is_none());
    assert_eq!(category("plain"), "NonVRChat");
    assert!(!in_all("plain"));
    // Tool is still current, yet undoing the move must not bring back the deleted category.
    let undone = c
        .command("undo", json!({"token":away["undoToken"]}))
        .unwrap();
    assert_eq!(undone["conflicts"], json!(["hair"]));
    assert_eq!(category("hair"), "Tool");
}
