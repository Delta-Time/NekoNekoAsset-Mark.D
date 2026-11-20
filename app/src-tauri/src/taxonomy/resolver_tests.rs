
use super::*;

#[test]
fn uncertain_base_choices_require_explicit_identity_selection() {
    let mut fallback = base("product:authored-fallback", "星猫 / 海猫", "");
    fallback["nameKind"] = json!("fallback");
    fallback["aliases"] = json!(["Authored alias"]);
    let mut bundle = base("product:authored-bundle", "苺佳 and GLC and 蒼葉", "");
    bundle["nameKind"] = json!("bundle");
    let resolver = AvatarTagResolver::from_options(&[fallback, bundle]);
    for tag in [
        "星猫 / 海猫",
        "Authored alias",
        "苺佳 and GLC and 蒼葉",
        "苺佳",
        "GLC",
    ] {
        assert!(resolver.resolve_tag(tag).is_none(), "{tag}");
    }
    assert!(resolver
        .resolve_manual_avatar("product:authored-fallback")
        .is_some());
    assert!(resolver
        .resolve_manual_avatar("product:authored-bundle")
        .is_some());
}

fn base(id: &str, original: &str, english: &str) -> Value {
    json!({"id":id,"officialName":original,"englishName":english,"displayName":format!("{original} / {english}"),"aliases":[format!("product:{id}"),"123456"]})
}

#[test]
fn uncurated_owned_names_resolve_complete_exact_tags_only() {
    let resolver =
        AvatarTagResolver::from_options(&[base("product:synthetic-cat", "ソラネコ", "Soraneko")]);
    for tag in [
        "ソラネコ",
        "ＳＯＲＡＮＥＫＯ",
        "Soraneko / ソラネコ",
        "ソラネコ(Soraneko)",
        "ソラネコ -Soraneko-",
        "【ソラネコ】対応",
        "オリジナル3Dモデル「ソラネコ」",
        "Soraneko전용",
    ] {
        let candidate = resolver.resolve_tag(tag).unwrap();
        assert_eq!(candidate["id"], "product:synthetic-cat", "{tag}");
        assert_eq!(candidate["entitlementEstablished"], false);
        assert_eq!(resolver.canonical_tag(tag), "product:synthetic-cat");
    }
    for tag in [
        "ソラネコ用コート",
        "Soraneko outfit",
        "Soraneko - coat",
        "ソラネコ / Coat",
        "ソラネコ / 海ネコ",
        "Soranekoish",
        "123456",
        "product:synthetic-cat",
        "booth:123456",
        "https://example.test/Soraneko",
    ] {
        assert!(resolver.resolve_tag(tag).is_none(), "{tag}");
    }
}

#[test]
fn honorific_tags_and_rejections_resolve_all_complete_known_names() {
    let mut sky = base("product:sky", "ソラネコ", "Soraneko");
    sky["aliases"] = json!(["Star Cat"]);
    let sea = base("product:sea", "ウミネコ", "Umineko");
    let resolver = AvatarTagResolver::from_options(&[sky, sea]);
    for (id, names) in [
        ("product:sky", vec!["ソラネコ", "Soraneko", "Star Cat"]),
        ("product:sea", vec!["ウミネコ", "Umineko"]),
    ] {
        let rejected = BTreeSet::from([resolver.canonical_tag(names[0])]);
        for name in names {
            for suffix in ["", "対応", "用", "専用"] {
                let tag = format!("{name}ちゃん{suffix}");
                let candidate = resolver.resolve_tag(&tag).unwrap();
                assert_eq!(candidate["id"], id, "{tag}");
                assert_eq!(candidate["basis"], "exactTag");
                assert_eq!(candidate["entitlementEstablished"], false);
                assert!(rejected.contains(&resolver.canonical_tag(&tag)), "{tag}");
                assert_eq!(resolver.canonical_tag(&tag), resolver.canonical_tag(name));
            }
        }
    }
    for tag in [
        "ソラネコちゃんちゃん",
        "ソラネコちゃんちゃん対応",
        "ソラネコちゃん対応用",
        "ソラネコちゃん用コート",
        "ソラネコちゃん衣装",
        "ソラネコちゃんが好き",
        "ソラネコ ちゃん",
        "Soranekoishちゃん",
        "未登録ちゃん",
        "ちゃん",
        "123456ちゃん",
        "product:skyちゃん",
        "ソラネコちゃん / ウミネコちゃん",
    ] {
        assert!(resolver.resolve_tag(tag).is_none(), "{tag}");
    }
}

#[test]
fn honorific_full_name_and_stripped_name_collisions_are_never_chosen() {
    let ordinary = base("booth:synthetic-curated", "ソラネコ", "Soraneko");
    let honorific = base("product:synthetic-owned", "ソラネコちゃん", "Little Sky");
    for options in [
        vec![ordinary.clone(), honorific.clone()],
        vec![honorific, ordinary.clone()],
        vec![
            ordinary,
            base("product:other-shop", "ソラネコ", "Other Sky"),
        ],
    ] {
        let resolver = AvatarTagResolver::from_options(&options);
        for tag in [
            "ソラネコちゃん",
            "ソラネコちゃん対応",
            "ソラネコちゃん用",
            "ソラネコちゃん専用",
        ] {
            assert!(resolver.resolve_tag(tag).is_none(), "{tag}");
            assert_eq!(resolver.canonical_tag(tag), normalize(tag));
        }
    }
}

#[test]
fn shared_names_are_ambiguous_but_distinct_bilingual_pairs_can_resolve() {
    let a = base("product:shop-a", "ソラネコ", "Starcat");
    let b = base("product:shop-b", "ソラネコ", "Skycat");
    for options in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
        let resolver = AvatarTagResolver::from_options(&options);
        assert!(resolver.resolve_tag("ソラネコ").is_none());
        assert!(resolver.resolve_tag("【ソラネコ】対応").is_none());
        assert_eq!(resolver.canonical_tag("ソラネコ"), "ソラネコ");
        assert_eq!(
            resolver.resolve_tag("ソラネコ / Starcat").unwrap()["id"],
            "product:shop-a"
        );
        assert_eq!(
            resolver.resolve_tag("ソラネコ / Skycat").unwrap()["id"],
            "product:shop-b"
        );
        assert!(resolver.resolve_tag("Starcat / Skycat").is_none());
    }
    let resolver = AvatarTagResolver::from_options(&[a.clone(), a]);
    assert!(resolver.resolve_tag("ソラネコ").is_some());
}

#[test]
fn a_special_edition_keeps_its_plain_product_name_unambiguous() {
    let plain = base("product:plain", "ソラネコ", "Soraneko");
    let mut edition = base("product:edition", "ソラネコ", "Soraneko");
    edition["labelQualifiers"] = json!(["Gen3.0"]);
    edition["displayName"] = json!("ソラネコ / Soraneko (Gen3.0)");
    let mut other_edition = edition.clone();
    other_edition["id"] = json!("product:other-edition");
    for options in [
        [plain.clone(), edition.clone()],
        [edition.clone(), plain.clone()],
    ] {
        let resolver = AvatarTagResolver::from_options(&options);
        assert_eq!(
            resolver.resolve_tag("ソラネコ").unwrap()["id"],
            "product:plain"
        );
        assert_eq!(
            resolver.resolve_manual_avatar("product:edition").unwrap()["id"],
            "product:edition"
        );
    }
    // Only an edition of its own name: still that product, or ambiguous among editions.
    assert_eq!(
        AvatarTagResolver::from_options(&[edition.clone()])
            .resolve_tag("ソラネコ")
            .unwrap()["id"],
        "product:edition"
    );
    assert!(AvatarTagResolver::from_options(&[edition, other_edition])
        .resolve_tag("ソラネコ")
        .is_none());
}

#[test]
fn another_listing_of_one_avatar_keeps_the_plain_listing_name_unambiguous() {
    let options = super::super::owned_base_options(&[
        json!({"id":"free","category":"Base","sourceTitle":"【無料版】オリジナル3Dモデル「ソラネコ」"}),
        json!({"id":"plain","category":"Base","sourceTitle":"オリジナル3Dモデル「ソラネコ」"}),
        json!({"id":"deluxe","category":"Base","sourceTitle":"[DX版]ソラネコ"}),
    ]);
    let resolver = AvatarTagResolver::from_options(&options);
    assert_eq!(
        resolver.resolve_tag("ソラネコ").unwrap()["id"],
        "product:plain"
    );
    // Without a plain listing the name stays ambiguous among the others.
    let options = super::super::owned_base_options(&[
        json!({"id":"free","category":"Base","sourceTitle":"【無料版】オリジナル3Dモデル「ソラネコ」"}),
        json!({"id":"deluxe","category":"Base","sourceTitle":"[DX版]ソラネコ"}),
    ]);
    assert!(AvatarTagResolver::from_options(&options)
        .resolve_tag("ソラネコ")
        .is_none());
}

#[test]
fn aliases_and_rejections_share_identity_without_overriding_collisions() {
    let mut a = base("product:synthetic-cat", "ソラネコ", "Soraneko");
    a["aliases"] = json!(["Star Cat", "Old Cat"]);
    let resolver = AvatarTagResolver::from_options(&[a.clone()]);
    let rejected = BTreeSet::from([resolver.canonical_tag("Old Cat")]);
    assert!(rejected.contains(&resolver.canonical_tag("ソラネコ対応")));
    assert!(rejected.contains(&resolver.canonical_tag("Star Cat")));
    let mut b = base("product:other-cat", "ウミネコ", "Umineko");
    b["aliases"] = json!(["Old Cat"]);
    let collision = AvatarTagResolver::from_options(&[a, b]);
    assert!(collision.resolve_tag("Old Cat").is_none());
    assert_eq!(collision.canonical_tag("Old Cat"), "old cat");
}

#[test]
fn curated_and_owned_collisions_do_not_choose_dictionary_order() {
    let known = base("booth:synthetic-known", "真冬", "Mafuyu");
    let owned = base("product:synthetic-other", "真冬", "Mafuyu");
    let resolver = AvatarTagResolver::from_options(&[known, owned]);
    for tag in ["真冬", "Mafuyu", "真冬 / Mafuyu"] {
        assert!(resolver.resolve_tag(tag).is_none());
        assert_eq!(resolver.canonical_tag(tag), normalize(tag));
    }
}

#[test]
fn manual_ids_resolve_exact_identity_without_enabling_automatic_id_tags() {
    let a = base("product:avatar-a", "ソラネコ", "Soraneko");
    let b = base("product:avatar-b", "ソラネコ", "Soraneko");
    let resolver = AvatarTagResolver::from_options(&[a, b]);
    assert!(resolver.resolve_tag("product:avatar-b").is_none());
    let selected = resolver.resolve_manual_avatar("product:avatar-b").unwrap();
    assert_eq!(selected["id"], "product:avatar-b");
    assert_eq!(selected["displayName"], "ソラネコ / Soraneko");
    assert!(resolver.resolve_manual_avatar("ソラネコ").is_none());
    assert!(resolver.resolve_manual_avatar("product:absent").is_none());
}
