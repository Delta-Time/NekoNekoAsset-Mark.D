
#[test]
fn gumroad_requires_explicit_vrchat_in_source_title_or_description() {
    for (category, title, description) in [
        (
            "Shader",
            "Authored paint shader for Blender",
            "A rendering material.",
        ),
        (
            "World",
            "Authored Unreal classroom",
            "An environment tutorial.",
        ),
        (
            "Clothes",
            "Authored clothing mesh",
            "For Unity and other engines.",
        ),
        (
            "Base",
            "Original 3D Avatar Soraneko",
            "A generic 3D character.",
        ),
        (
            "Udon",
            "Authored script",
            "Includes tools and VR controller models.",
        ),
        (
            "Shader",
            "Authored shader",
            "VRChatters and avrcollision are unrelated words.",
        ),
    ] {
        let source = json!({"store":"Gumroad","sourceTitle":title,
                "sourceDescription":description,"sourceCategory":{"name":category,"parentName":"VRChat"},
                "sourceTags":["VRChat","VRC","Unity","Avatar","Shader","Udon"]});
        let unchanged = source.clone();
        let inference = infer_product_category(&source);
        assert_eq!(inference["category"], "NonVRChat", "{title}");
        assert_eq!(inference["scope"], "other", "{title}");
        assert_eq!(inference["scopeEvidence"]["vrchat"], false);
        assert_eq!(source, unchanged);
    }
    for (title, description, expected) in [
        ("[VRC] Authored Hair", "", "Hair"),
        ("Authored Shader", "Works with VRChat avatars.", "Shader"),
        ("ＶＲＣ対応 Authored Hair", "", "Hair"),
        ("Authored Hair", "Designed for vRc.", "Hair"),
        ("Authored Hair for VRチャット", "", "Hair"),
        ("Authored Hair (VRCFury)", "", "Hair"),
    ] {
        let inference = infer_product_category(&json!({"provider":"gumroad",
                "sourceTitle":title,"sourceDescription":description}));
        assert_eq!(inference["category"], expected, "{title}");
        assert_eq!(inference["scope"], "vrchat", "{title}");
    }
    let renamed = infer_product_category(&json!({"store":"Gumroad",
            "sourceTitle":"Authored Hair","title":"Human VRChat nickname"}));
    assert_eq!(renamed["category"], "NonVRChat");
}

#[test]
fn gumroad_text_gate_does_not_change_booth_or_jinxxy() {
    for store in ["BOOTH", "Jinxxy"] {
        let inference = infer_product_category(&json!({"store":store,
                "sourceTitle":"Authored Shader","sourceTags":["VRChat","Shader"]}));
        assert_eq!(inference["category"], "Shader", "{store}");
        assert_eq!(inference["scope"], "vrchat", "{store}");
    }
}

#[test]
fn jinxxy_scope_preserves_specific_types_and_falls_back_inside_vrchat() {
    let generic = infer_product_category(
        &json!({"provider":"jinxxy","sourceTitle":"Authored View (VRCFury)"}),
    );
    assert_eq!(generic["category"], "Gimmick");
    assert_eq!(generic["scope"], "vrchat");
    assert_eq!(generic["basis"], "providerFallback");
    let props = infer_product_category(
        &json!({"provider":"jinxxy","sourceTitle":"Authored View (VRCFury)","sourceCategory":{"name":"avatar-props","types":["avatar_addon"]},"sourceTags":["vrcfury","animation","xray","screenshader"]}),
    );
    assert_eq!(props["category"], "Accessory");
    assert_eq!(props["scope"], "vrchat");
    let addon = infer_product_category(
        &json!({"provider":"jinxxy","sourceTitle":"Authored View","sourceCategory":{"name":"avatars","types":["avatar_addon"]}}),
    );
    assert_eq!(addon["category"], "Gimmick");
    assert_eq!(addon["basis"], "sourceType");
    for (title, expected) in [
        ("Authored Dress (VRCFury)", "Clothes"),
        ("Authored Hair", "Hair"),
        ("Authored Texture", "Texture"),
        ("Authored Tool", "Tool"),
    ] {
        let value = infer_product_category(&json!({"store":"Jinxxy","sourceTitle":title}));
        assert_eq!(value["category"], expected);
        assert_eq!(value["scope"], "vrchat");
    }
    // Source default must never recategorize another store's unrelated files.
    assert_eq!(
        infer_product_category(&json!({"provider":"booth","sourceTitle":"Authored Texture"}))
            ["category"],
        "NonVRChat"
    );
}
#[test]
fn typography_does_not_discard_valid_readings_or_slice_unicode_indices() {
    for (input, expected) in [
        ("“そらねこ”", "Soraneko"),
        ("★そらねこ★", "Soraneko"),
        ("そら—ねこ", "Sora-neko"),
        ("シロネコ\u{fe0f}", "Shironeko"),
        ("ゐろは", "Iroha"),
        ("さゞなみ", "Sazanami"),
        ("くゝり", "Kukuri"),
        ("Étoile", "Etoile"),
    ] {
        assert_eq!(super::romanized_name(input), expected, "{input}");
    }
    assert_eq!(super::clean_avatar_name("İro Ver.2"), "İro");
    assert_eq!(super::clean_avatar_name("Silver2"), "Silver2");
    assert_eq!(super::clean_avatar_name("Version2"), "Version2");
    assert_eq!(
        super::avatar_label_parts("Version2"),
        ("Version2".into(), None)
    );
    assert_eq!(super::avatar_label_parts("“V2”"), ("V2".into(), None));
    assert_eq!(
        super::avatar_label_parts("Original 3D Avatar"),
        ("Original 3D Avatar".into(), None)
    );
    let unnamed = super::owned_base_options(&[
        serde_json::json!({"id":"synthetic-unnamed","category":"Base","sourceTitle":"Original 3D Avatar"}),
    ]);
    assert_eq!(unnamed.len(), 1);
    assert_eq!(unnamed[0]["id"], "product:synthetic-unnamed");
    assert_eq!(unnamed[0]["nameKind"], "fallback");
    assert_eq!(unnamed[0]["englishName"], "");
    assert_eq!(
        super::clean_avatar_name("Original 3D Modeler"),
        "Original 3D Modeler"
    );
    assert_eq!(super::romanized_name("İro"), "Iro");
    assert_eq!(super::romanize_kana("そらΩ"), "");
    assert_eq!(super::romanize_kana("そら龘"), "");
    assert_eq!(super::reading_input("★星空猫★"), "星空猫");
    #[cfg(target_os = "windows")]
    assert_eq!(super::romanized_name("★星空猫★"), "Hoshizoraneko");
}
use super::*;

#[test]
fn categories_preserve_precedence_and_unknown_fallback() {
    assert_eq!(infer_category(&Value::Null, &[])["category"], "NonVRChat");
    assert_eq!(
        infer_category(&json!({"name":"3Dモデル（その他）"}), &[])["basis"],
        "fallback"
    );
    let source = json!({"name":"アバター", "parentName":"3D", "id":null});
    let result = infer_category(&source, &["衣装".into()]);
    assert_eq!(result["category"], "Clothes");
    assert_eq!(result["basis"], "tags");
    assert_eq!(source["name"], "アバター");
    assert!(result.get("ownedAvatars").is_none());
    assert_eq!(
        infer_category(
            &json!({"name":"3D衣装", "parentName":"アバター"}),
            &["shader".into()]
        )["category"],
        "Clothes"
    );
    assert_eq!(
        infer_category(&json!("３Ｄキャラクター"), &[])["category"],
        "Base"
    );
    assert_eq!(
        infer_category(&Value::Null, &["chair".into(), "worldwide".into()])["category"],
        "NonVRChat"
    );
}

#[test]
fn every_fixed_category_is_reachable_without_ownership() {
    let examples = [
        "avatar",
        "clothes",
        "hair",
        "props",
        "textures",
        "motion",
        "tool",
        "shader",
        "gimmick",
        "world",
        "udon",
        "illustration",
    ];
    for (category, example) in CATEGORY_KEYS.iter().zip(examples) {
        assert_eq!(infer_category(&json!(example), &[])["category"], *category);
    }
}

#[test]
fn nfkc_aliases_and_boundaries_do_not_guess_ids() {
    assert!(aliases("Ｍａｆｕｙｕ").contains(&"真冬".into()));
    assert!(aliases("ﾏﾇｶ対応").contains(&"manuka".into()));
    assert_eq!(match_avatars("Mafuyu_coat.zip")[0]["id"], "booth:5007531");
    assert_eq!(match_avatars("真冬用・マヌカ対応").len(), 2);
    assert!(match_avatars("萌芽").is_empty());
    assert!(match_avatars("minted manukau limelight").is_empty());
    assert!(match_avatars("5007531").is_empty());
    assert!(match_avatars("").is_empty());
    assert_eq!(match_avatars("Mafuyu")[0]["entitlementEstablished"], false);
}

#[test]
fn rejection_blocks_equivalent_alias_after_refresh() {
    assert_eq!(canonical_tag("真冬対応"), canonical_tag("Ｍａｆｕｙｕ"));
    assert_eq!(canonical_tag("真冬 / Mafuyu"), canonical_tag("真冬"));
    assert_ne!(canonical_tag("真冬 / マヌカ"), canonical_tag("真冬"));
    assert_ne!(canonical_tag("Mafuyu coat"), canonical_tag("Mafuyu"));
    assert_eq!(
        effective_tags(
            &["Mafuyu".into(), "まふゆ".into(), "new".into()],
            &["manual".into()],
            &["真冬".into()]
        ),
        vec!["manual", "new"]
    );
    assert!(tag_suggestions("casual R18", &["カジュアル".into(), "R-18".into()]).is_empty());
}

#[test]
fn creator_spellings_and_legacy_readings_share_exact_compatibility_identity() {
    for (id, original, english, legacy) in [
        ("3443188", "竜胆", "Rindo", "Rindo"),
        ("3659436", "リーファ", "Leefa", "Riifa"),
        ("3564947", "あのん", "Anon", "Anon"),
        ("6373683", "りりか", "ririka", "Ririka"),
        ("6846646", "真央", "mao", "mao"),
        ("5650156", "しお", "Sio", "Shio"),
        ("2780069", "ここあ", "Kokoa", "Kokoa"),
    ] {
        for spelling in [
            original.to_owned(),
            english.to_owned(),
            legacy.to_owned(),
            format!("{original} / {english}対応"),
            format!("【{original}】対応"),
        ] {
            let candidate = exact_avatar_tag(&spelling).unwrap();
            assert_eq!(candidate["id"], format!("booth:{id}"));
            assert_eq!(candidate["englishName"], english);
            assert_eq!(candidate["englishNameStatus"], "official");
            assert_eq!(candidate["entitlementEstablished"], false);
        }
        assert!(exact_avatar_tag(&format!("{original}用コート")).is_none());
        assert_eq!(canonical_tag(legacy), canonical_tag(original));
        assert!(effective_tags(&[legacy.into()], &[], &[original.into()]).is_empty());
    }
}

#[test]
fn product_type_is_not_the_tools_output_or_avatar_compatibility() {
    // Authored structural equivalents of public 8643321 and 8606444;
    // no product-ID dispatch or copied descriptions.
    let addon = json!({"sourceCategory":"ソフトウェア", "sourceTitle":"Blenderアドオン：形状の生成", "sourceTags":["Blender","アバター"], "sourceDescription":"Blender addon for editing shapes. VRChat supports the exported shapes."});
    let maker = json!({"sourceCategory":{"name":"ソフトウェア"},"sourceTitle":"[Unity] Surface Mask Maker", "sourceTags":["テクスチャ","マテリアル"],"sourceDescription":"Unity editor tool that generates texture masks."});
    assert_eq!(infer_product_category(&addon)["category"], "Tool");
    assert_eq!(infer_product_category(&addon)["scope"], "vrchat");
    assert_eq!(infer_product_category(&maker)["category"], "Tool");
    assert_eq!(infer_product_category(&maker)["scope"], "unity");
    for output_tag in ["Hair", "Clothes", "Shader", "Motion", "アバター"] {
        let software = json!({"sourceCategory":"Software", "sourceTitle":"Unity Editor Extension", "sourceTags":["Unity",output_tag]});
        assert_eq!(
            infer_product_category(&software)["category"],
            "Tool",
            "{output_tag}"
        );
    }
    let shader = json!({"sourceCategory":"Software", "sourceTitle":"Unity Water Shader", "sourceTags":["Unity","Shader"]});
    assert_eq!(infer_product_category(&shader)["category"], "Shader");
    for source in [
        json!({"sourceTitle":"Blender add-on", "sourceTags":["Blender","アバター"]}),
        json!({"sourceTitle":"Shape helper", "sourceTags":["VRChat","アバター"]}),
        json!({"sourceTitle":"Example", "description":"Compatible with VRChat avatars and original 3D characters."}),
        json!({"sourceTitle":"8アバター対応", "sourceTags":["VRChat","アバター"]}),
        json!({"sourceTitle":"Avatar Configuration", "sourceTags":["VRChat","Avatar"]}),
        json!({"sourceTitle":"Example for Soraneko avatar", "sourceTags":["VRChat","Avatar"]}),
        json!({"sourceTitle":"アバター改変素材", "sourceTags":["VRChat","アバター"]}),
        json!({"sourceTitle":"アバター「ソラネコ」設定", "sourceTags":["VRChat","アバター"]}),
        json!({"sourceCategory":{"name":"その他","parentName":"3Dキャラクター"},"sourceTitle":"Example", "sourceTags":["VRChat"]}),
    ] {
        assert_eq!(
            infer_product_category(&source)["category"],
            "NonVRChat",
            "{source}"
        );
    }
    // Strong leaf/title product evidence survives, as does a real avatar
    // whose description merely names Blender or a required addon.
    for source in [
        json!({"sourceCategory":"3Dキャラクター", "sourceTitle":"ソラネコ", "sourceTags":["VRChat"]}),
        json!({"sourceTitle":"Original 3D Avatar Soraneko", "sourceTags":["VRChat"],"description":"Created in Blender. Requires an addon for an optional edit."}),
        json!({"sourceTitle":"Original 3D Avatar Soraneko for VRChat", "sourceTags":["VRChat","アバター"]}),
        json!({"sourceTitle":"オリジナル3Dモデル「ソラネコ」", "sourceTags":["VRChat","アバター"]}),
    ] {
        assert_eq!(
            infer_product_category(&source)["category"],
            "Base",
            "{source}"
        );
    }
    for (title, description) in [
        (
            "Example",
            "Requires a Unity editor extension to install this avatar.",
        ),
        ("Example", "Optional Blender addon is required for editing."),
    ] {
        assert_ne!(
            infer_product_category(
                &json!({"sourceTitle":title,"description":description,"sourceTags":["VRChat"]})
            )["category"],
            "Base"
        );
    }
    let raw = addon.clone();
    infer_product_category(&addon);
    assert_eq!(addon, raw);
}

#[test]
fn mishe_exact_aliases_infer_compatibility_without_entitlement() {
    for tag in [
        "ミーシェ",
        "Mishe",
        "Ｍｉｓｈｅ",
        "ﾐｰｼｪ",
        "Miishie",
        "ミーシェ / Mishe",
        "ミーシェ / Miishie",
        "ミーシェ対応",
        "オリジナル3Dモデル「ミーシェ」",
    ] {
        let candidate = exact_avatar_tag(tag).expect("exact Mishe alias");
        assert_eq!(candidate["id"], "booth:1256087", "{tag}");
        assert_eq!(candidate["displayName"], "ミーシェ / Mishe");
        assert_eq!(candidate["basis"], "exactTag");
        assert_eq!(candidate["entitlementEstablished"], false);
        assert_eq!(canonical_tag(tag), "booth:1256087");
    }
    for tag in [
        "Mishe coat",
        "ミーシェ用ジャケット",
        "ミーシェ / 真冬",
        "Mishe / jacket",
        "booth:1256087",
        "1256087",
    ] {
        assert!(exact_avatar_tag(tag).is_none(), "not an exact alias: {tag}");
    }
    assert!(aliases("Miishie").contains(&"mishe".to_owned()));
    assert_eq!(romanized_name("ミーシェ"), "Mishe");
    assert_eq!(romanized_name("Miishie"), "Mishe");
    let dictionary = dictionary();
    let entry = dictionary["avatars"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["productId"] == "1256087")
        .unwrap();
    assert_eq!(entry["englishName"], "Mishe");
    assert_eq!(entry["englishNameStatus"], "userAccepted");
    assert_eq!(
        entry["englishNameEvidence"]["url"],
        "https://booth.pm/ja/items/2873084"
    );
    assert_eq!(
        effective_tags(
            &["Mishe".into(), "ミーシェ".into()],
            &["Miishie".into()],
            &[]
        ),
        vec!["Miishie"]
    );
    assert!(effective_tags(&["Mishe".into()], &[], &["ミーシェ".into()]).is_empty());
}

#[test]
fn cached_exact_lookup_preserves_alias_pairs_suffixes_and_boundaries() {
    for spelling in [
        "真冬",
        "Ｍａｆｕｙｕ",
        "真冬 / Mafuyu",
        "Mafuyu (真冬)",
        "真冬 -Mafuyu-",
        "真冬『Mafuyu』",
        "Mafuyu対応",
        "ミルク Re / Milk Re",
    ] {
        let candidate = exact_avatar_tag(spelling).unwrap();
        let expected = if spelling.contains("Milk Re") {
            "booth:2953391"
        } else {
            "booth:5007531"
        };
        assert_eq!(candidate["id"], expected, "{spelling}");
        assert_eq!(canonical_tag(spelling), expected);
    }
    for spelling in [
        "Mafuyu_coat.zip",
        "Mafuyu / マヌカ",
        "萌芽",
        "minted",
        "Milk Re jacket",
    ] {
        assert!(exact_avatar_tag(spelling).is_none(), "{spelling}");
    }
    assert_eq!(clean_avatar_name("真冬 / Mafuyu"), "真冬");
    assert_eq!(romanized_name("真冬 / Mafuyu"), "Mafuyu");
}

#[test]
fn booth_character_category_outranks_tags_unless_software_is_declared() {
    // Authored structural equivalents; the human chose the category alone,
    // so a gimmick or pet filed there follows it too.
    for (title, tag) in [
        ("星猫オリジナル3Dモデル「ソラネコ」-soraneko-", "髪型"),
        (
            "オリジナル3Dアバター『星空ソラネコ』 #ソラネコ3D",
            "ギミック",
        ),
        (
            "【VRChat 】オリジナルコラボ3Dアバター SORA_NEKO",
            "アニメーション",
        ),
        (
            "ソラネコ オリジナル3Dモデル -VRChatアバター & VRM & 追加衣装",
            "衣装",
        ),
        (
            "オリジナル3Ｄモデル「ソラネコ」Soraneko v1.17",
            "lens texture",
        ),
        ("月猫オリジナル3Dモデル「ソラネコ」", "シェーダー"),
        ("ペットウサギ「ソラネコ」", "vrc想定アクセサリー"),
        ("Authored Open Car", "アバターギミック"),
        ("Authored×Summer", "イラスト"),
    ] {
        for category in [json!({"name":"3Dキャラクター"}), json!("３Ｄキャラクター")]
        {
            let result = infer_product_category(
                &json!({"sourceCategory":category,"sourceTitle":title,"sourceTags":[tag]}),
            );
            assert_eq!(result["category"], "Base", "{title}");
            assert_eq!(result["basis"], "sourceCategory", "{title}");
        }
    }
    // A software tag alone is one more tag.
    assert_eq!(
        infer_product_category(
            &json!({"sourceCategory":{"name":"3Dキャラクター"},"sourceTitle":"Authored","sourceTags":["プラグイン"]})
        )["category"],
        "Base"
    );
    for source in [
        json!({"sourceCategory":{"name":"3Dキャラクター"},"sourceTitle":"「ソラネコ」用R18体Body Add-on v1.1","sourceTags":["VRChat","アバター"]}),
        json!({"sourceCategory":{"name":"3Dキャラクター"},"sourceTitle":"Authored","sourceTags":["VRChat"],"sourceDescription":"Unity editor tool for avatars."}),
    ] {
        assert_eq!(
            infer_product_category(&source)["category"],
            "Tool",
            "{source}"
        );
    }
    // Only this exact store category: not a parent, and not Jinxxy's own names.
    for (source, expected) in [
        (
            json!({"sourceCategory":{"name":"その他","parentName":"3Dキャラクター"},"sourceTitle":"Authored","sourceTags":["髪型"]}),
            "Hair",
        ),
        (
            json!({"sourceCategory":{"name":"avatars"},"sourceTitle":"Authored","sourceTags":["hair"]}),
            "Hair",
        ),
        (
            json!({"sourceCategory":{"name":"3D衣装"},"sourceTitle":"Authored","sourceTags":["アバター"]}),
            "Clothes",
        ),
    ] {
        assert_eq!(
            infer_product_category(&source)["category"],
            expected,
            "{source}"
        );
    }
    // Managed paths freeze this lower layer's answer, so it must not move.
    assert_eq!(
        infer_category(&json!({"name":"3Dキャラクター"}), &["髪型".into()])["category"],
        "Hair"
    );
    // Gumroad's explicit VRChat text requirement still has the last word.
    assert_eq!(
        infer_product_category(
            &json!({"provider":"gumroad","sourceCategory":{"name":"3Dキャラクター"},"sourceTitle":"Authored"})
        )["category"],
        "NonVRChat"
    );
}

#[test]
fn title_alone_makes_a_base_only_when_its_wording_places_one_model_name() {
    let without_store_category = |title: &str, extra: &Value| {
        let mut source = json!({"sourceTitle":title});
        for (key, value) in extra.as_object().unwrap() {
            source[key] = value.clone();
        }
        infer_product_category(&source)
    };
    let generic = json!({"sourceCategory":{"name":"3Dモデル（その他）"}});
    for title in [
        "オリジナル3Dモデル「ソラネコ」",
        "オリジナル3Dモデル 『Soraneko -ソラネコ-』",
        "オリジナル 3Dモデル『ソラネコ』",
        "『ソラネコ』オリジナル3Dモデル",
        "AuthoredCats vol.2『ソラネコ』オリジナル3Dモデル",
        "【非公開】【オリジナル3Dモデル】Soraneko-ソラネコ-",
        "【オリジナル3Dモデル】ソラネコ(フルセット)",
        "◆オリジナル3Dモデル◆ソラネコ ver1.0 (通常版)",
        "Original 3D Model ［soraneko -ソラネコ-］",
        "※11/30非公開予定 オリジナル3Dモデル「Soraneko〈ソラネコ〉」",
        "非公開済＠オリジナル3Dモデル「Soraneko〈ソラネコ〉」（Quest版付属）",
        "【ARCHIVE】【VRChat想定 オリジナル3Dモデル 『ソラネコ Soraneko 』フルセット版",
        "【VRChat対応 オリジナル3Dモデル 『ソラネコ -フルセット版-』v1.1＆V2.0】",
        "オリジナル3Dモデル『ソラネコ』+追加衣装:Authored",
        "VRChat対応オリジナル3Dモデル「ソラネコ」",
        "VRChat用3Dモデル「ソラネコ(そらねこ)」",
        "VRChat向けモデル【ソラネコ Ver1.1】",
        "VRC向け3Dモデル【ソラネコ -Soraneko- 】V2",
        "OculusQuest VRChat対応 3Dモデル 『ソラネコ -フルセット版-』v1.91",
        "[VRChat向けアバター]ソラネコ・ノワール",
        "[VRChat向けアバター] SORANEKO - 空猫",
    ] {
        for extra in [&json!({}), &generic] {
            let result = without_store_category(title, extra);
            assert_eq!(result["category"], "Base", "{title} {extra}");
            assert_eq!(result["basis"], "sourceTitle", "{title} {extra}");
        }
    }
    // Tags and descriptions, which often list contents, do not outrank the title.
    assert_eq!(
        without_store_category(
            "オリジナル3Dモデル「ソラネコ」",
            &json!({"sourceCategory":{"name":"3Dモデル（その他）"},"sourceTags":["シェーダー"],"sourceDescription":"VRChat 向け。衣装とテクスチャを含みます。"})
        )["category"],
        "Base"
    );
    for title in [
        // The phrase can end a garment's title, or sit beside a garment's name.
        "【version1.1】ふわふわワンピース【オリジナル3Dモデル】",
        "『ソラT01』Ver2.0【オリジナル3Dモデル】アバター追加衣装",
        "【オリジナル3Dモデル】ソラネコ 素体用追加テクスチャ",
        "オリジナル3Dモデル「ソラネコ」専用コート",
        "オリジナル3Dモデル「ソラネコ」用ヘアスタイル",
        "ソラネコ対応オリジナル3Dモデル",
        "オリジナル3Dモデル「ソラネコ」 Add-on",
        "オリジナル3Dモデル「ソラネコ」用ワンピース",
        "オリジナル3Dモデル ソラネコ&ウミネコ専用衣装",
        "オリジナル3Dモデル用メガネ",
        // A bare 3Dモデル also labels props.
        "【3Dモデル】洗面器",
        "[３Dモデル]単三電池 [VRC]",
        "【VRChat向け】オルゴール 【3Dモデル】",
        "VRChat向けスプーン",
        // Platform wording followed by another product type.
        "VRChat向けアバターギミック",
        "VRChat向けアバター用ヘアピン",
        "【VRC想定アバターLOADING時に】アニメーションファイル",
        // No name, or not this phrase.
        "オリジナル3Dモデル",
        "オリジナル3Dモデル ver1.0",
        "Original 3D Modeler",
        "Original 3D Modeler Soraneko",
        "「ソラネコ」「ウミネコ」オリジナル3Dモデル",
    ] {
        for extra in [&json!({}), &generic] {
            assert_ne!(
                without_store_category(title, extra)["category"],
                "Base",
                "{title} {extra}"
            );
        }
    }
    // A specific store category wins, and a removed page itself proves nothing.
    for (leaf, expected) in [
        ("3D衣装", "Clothes"),
        ("3D小道具", "Accessory"),
        ("3Dテクスチャ", "Texture"),
    ] {
        let result = without_store_category(
            "オリジナル3Dモデル「ソラネコ」",
            &json!({"sourceCategory":{"name":leaf},"sourceTags":["VRChat"]}),
        );
        assert_eq!(result["category"], expected, "{leaf}");
    }
    assert_eq!(
        without_store_category(
            "Authored Soraneko",
            &json!({"publicPageAvailability":"unavailable"})
        )["category"],
        "NonVRChat"
    );
    // Store-specific gates keep their last word.
    let gumroad = json!({"provider":"gumroad"});
    assert_eq!(
        without_store_category("オリジナル3Dモデル「ソラネコ」", &gumroad)["category"],
        "NonVRChat"
    );
    assert_eq!(
        without_store_category(
            "オリジナル3Dモデル「ソラネコ」",
            &json!({"provider":"gumroad","sourceDescription":"For VRChat."})
        )["category"],
        "Base"
    );
    assert_eq!(
        without_store_category(
            "Original 3D Model Soraneko",
            &json!({"provider":"jinxxy","sourceCategory":{"name":"avatars","types":["avatar_addon"]}})
        )["category"],
        "Gimmick"
    );
    assert_eq!(
        without_store_category(
            "Original 3D Model Soraneko",
            &json!({"provider":"jinxxy","sourceCategory":{"name":"","types":["avatar_addon"]}})
        )["category"],
        "Gimmick"
    );
}

#[test]
fn public_classification_complaints_use_specific_type_and_scope() {
    // Independently authored minimal public-metadata patterns; no private
    // catalogue records, descriptions or paid file lists are included.
    let cases = [
        (
            "7329831",
            "ソフトウェア",
            "BeatPad",
            vec!["ツール", "Windows", "AfterEffects", "AE"],
            "NonVRChat",
            "other",
        ),
        (
            "7969637",
            "ソフトウェア",
            "NAI Smart Studio",
            vec!["Windows", "NovelAI", "生成AI"],
            "NonVRChat",
            "other",
        ),
        (
            "3282463",
            "イラスト素材",
            "配信デスク素材",
            vec!["VTuber", "配信枠", "配信素材"],
            "NonVRChat",
            "other",
        ),
        (
            "2153720",
            "ボイス",
            "声ネタ",
            vec!["ボイス", "サンプリング素材", "VTuber"],
            "NonVRChat",
            "other",
        ),
        (
            "1906822",
            "素材（その他）",
            "星空の表紙素材",
            vec!["小説表紙素材", "同人誌表紙"],
            "NonVRChat",
            "unknown",
        ),
        (
            "2662823",
            "素材（その他）",
            "炎 APNG",
            vec!["trpg素材", "trpg立ち絵素材"],
            "NonVRChat",
            "other",
        ),
        (
            "4070235",
            "フォント・書体",
            "テロッププリセット",
            vec!["AE", "aftereffcts", "テロップ"],
            "NonVRChat",
            "other",
        ),
        (
            "2826022",
            "素材（その他）",
            "Deco-UI",
            vec!["Unity", "ゲーム素材", "VRChat", "シェーダー"],
            "Shader",
            "vrchat",
        ),
        (
            "2832898",
            "素材（その他）",
            "ExWire",
            vec!["Unity", "VRChat", "シェーダー"],
            "Shader",
            "vrchat",
        ),
        (
            "3776089",
            "素材（その他）",
            "スケーティングロード",
            vec![
                "アニメーション",
                "Unity",
                "VRChat",
                "Humanoid",
                "Emote",
                "モーション素材",
            ],
            "Motion",
            "vrchat",
        ),
        (
            "3799864",
            "素材（その他）",
            "AFK 足組み",
            vec![
                "アニメーション",
                "Unity",
                "VRChat",
                "Humanoid",
                "Emote",
                "モーションデータ",
            ],
            "Motion",
            "vrchat",
        ),
    ];
    for (id, category, title, tags, expected, scope) in cases {
        let result = infer_product_category(
            &json!({"sourceCategory":{"name":category},"sourceTitle":title,"sourceTags":tags}),
        );
        assert_eq!(result["category"], expected, "public pattern {id}");
        assert_eq!(result["scope"], scope, "public scope {id}");
    }
    assert_eq!(
        infer_product_category(&json!({"sourceCategory":"素材（その他）","sourceTags":["VRChat"]}))
            ["category"],
        "NonVRChat"
    );
    for (tag, expected) in [("Shader", "Shader"), ("Motion", "Motion")] {
        assert_eq!(
            infer_product_category(
                &json!({"sourceCategory":"Materials","sourceTags":["Unity",tag]})
            )["category"],
            expected
        );
    }
}

fn booth_listing(parent: &str, leaf: &str, title: &str, tags: &[&str]) -> Value {
    json!({"store":"BOOTH","sourceCategory":{"name":leaf,"parentName":parent},"sourceTitle":title,"sourceTags":tags})
}

#[test]
fn booth_categories_and_what_a_product_is_built_on_decide_its_type() {
    // Independently authored structures of what sellers file in each category.
    for (leaf, title, tags, expected) in [
        (
            "3D環境・ワールド",
            "【販売ワールド】Authored Hotel",
            &["VRChat", "Udon", "シェーダー", "ギミック"][..],
            "World",
        ),
        (
            "3D環境・ワールド",
            "【販売ワールド】Authored Bar ギミック シェーダー",
            &["VRChat", "Udon"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Room",
            &["VRChat", "Udon", "UdonSharp"],
            "World",
        ),
        ("3D環境・ワールド", "Authored Room", &["小物"], "World"),
        (
            "3D環境・ワールド",
            "Authored ワールド",
            &["シェーダー", "小物"],
            "World",
        ),
        // Built for a world, an avatar or the renderer, whatever the store called it.
        (
            "3D環境・ワールド",
            "Authored Light",
            &["VRChat", "シェーダー"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "Authored フォグパーティクル",
            &["VRChat"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "AuthoredVolumetricFog",
            &["VRChat"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "Authored Table",
            &["VRChat", "ワールドギミック"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "[CVS2] Authored Car",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Meter",
            &["チクワカーシステム"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "【ワールド用】Authored 表示ギミック",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored アバターギミック",
            &["VRChat"],
            "Gimmick",
        ),
        (
            "3D環境・ワールド",
            "【VRC向けワールド】Authored",
            &["VRChat", "ワールドギミック", "Udon"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Room",
            &["VRChat", "VRChatワールド"],
            "World",
        ),
        // A place keeps its tags to itself, and a word beside "with" is not what is sold.
        (
            "3D環境・ワールド",
            "Authored Hotel【PCVR／Quest】",
            &["部屋", "ホームワールド", "Udon", "ワールドギミック"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Beach Club",
            &["VRChatワールド", "シェーダー"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "VRChat World Authored Mansion udon",
            &["家具", "ワールド", "udon"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "[Udon] Authored Door",
            &["Udon", "家具"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Kotatsu",
            &["インテリア", "ワールドギミック"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Clock【VRChat Worldギミック】",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "VRChat World用 Authored Water Shader",
            &["VRChat"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "Authored アバターペデスタルギミック",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Tail",
            &["アバターギミック", "ModularAvatar"],
            "Gimmick",
        ),
        (
            "3D環境・ワールド",
            "【VRChatアバターギミック】Authored パーティクル",
            &["VRChat"],
            "Gimmick",
        ),
        (
            "3D環境・ワールド",
            "VRChat World Authored Heights",
            &["家具", "ワールドギミック"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Cafe with Volumetric Lighting",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "【VRCワールド】Authored Stage パーティクル演出",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Room【Udon不使用】",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "VRChatワールド軽量化ツール",
            &["VRChat"],
            "Tool",
        ),
        (
            "3D環境・ワールド",
            "【Udon】Authored 花火パーティクル",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "プリ機 ワールドギミック Authored",
            &["インテリア", "ワールド"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored しっぽギミック",
            &["アバターギミック", "ModularAvatar"],
            "Gimmick",
        ),
        (
            "3D環境・ワールド",
            "アバター用 Authored ギミック",
            &["VRChat"],
            "Gimmick",
        ),
        (
            "3D環境・ワールド",
            "Authored ギミック付き家電",
            &["VRChat", "ギミック"],
            "World",
        ),
        (
            "3Dツール・システム",
            "Authored NPC セットアップツール",
            &["VRChat", "ワールドアセット"],
            "Udon",
        ),
        (
            "3Dツール・システム",
            "【ワールド用】Authored 時計ギミック",
            &["VRChat"],
            "Udon",
        ),
        (
            "3Dツール・システム",
            "Authored Addon",
            &["VRChat", "VRChatギミック"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "Authored Scene Cleaner Tool",
            &["VRChat", "Udon", "エディタ拡張"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored Scene Cleaner",
            &["VRChat", "UdonSharp", "エディタ拡張"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "ギミック作成ツール Authored",
            &["VRChat", "Udon"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored 撫でギミック",
            &["VRChat", "UnityEditor"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "【MA対応】Authored Tailor",
            &["VRChat", "ツール"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "UdonSharp Refactor Tool",
            &["VRChat", "UdonSharp"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "「Authored つく～る」アバターギミックを作るギミック",
            &["VRChat", "エディター拡張", "ツール"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "【Authored】【ModularAvatar対応】",
            &["VRChat"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "Authored VRM Exporter",
            &["VRChat", "VRMA対応"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored Tracker",
            &["VRChat", "ツール", "ギミック", "ModularAvatar"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "Authored Mesh",
            &["VRChat", "便利ツール", "MA対応"],
            "Tool",
        ),
        (
            "3Dモデル（その他）",
            "Authored Table",
            &["VRChat", "ワールドギミック"],
            "Udon",
        ),
        (
            "3Dツール・システム",
            "Authored Fixer",
            &["VRChat", "ModularAvatar", "Editor拡張"],
            "Tool",
        ),
        (
            "ソフトウェア",
            "Authored Panel",
            &["VRChat", "udonギミック"],
            "Udon",
        ),
        (
            "ソフトウェア",
            "Authored script",
            &["Udon", "UdonSharp"],
            "Udon",
        ),
        (
            "ソフトウェア",
            "Authored Kit",
            &["VRChat", "ModularAvatar対応"],
            "Gimmick",
        ),
        (
            "ソフトウェア",
            "Authored Light Tool",
            &["VRChat", "ModularAvatar"],
            "Tool",
        ),
        (
            "ソフトウェア",
            "Authored Volumetric Light",
            &["VRChat", "Unity"],
            "Shader",
        ),
        (
            "ソフトウェア",
            "Authored Camera",
            &["VRChat", "ギミック"],
            "Tool",
        ),
        (
            "3D小道具",
            "VRChat Udon用ギミック「Authored」",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D小道具",
            "【VRChat向け】Authored Drink【ワールド小物】",
            &["VRChat", "アバター用小物"],
            "World",
        ),
        (
            "3D小道具",
            "Authored Coffee セット（Worldギミック）",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D小道具",
            "Authored Underworld Props",
            &["VRChat"],
            "Accessory",
        ),
        (
            "3D小道具",
            "【ワールド小物】Authored Add-on",
            &["VRChat"],
            "World",
        ),
        (
            "ゲーム関連商品",
            "Authored Cafe World Prefab",
            &["VRChat"],
            "World",
        ),
        (
            "3Dモデル（その他）",
            "Authored Sign",
            &["VRChat", "ワールド", "3dモデル 小物"],
            "World",
        ),
        (
            "3Dモデル（その他）",
            "Authored Cup",
            &["VRChat", "ワールド", "小物", "アバター"],
            "Accessory",
        ),
        (
            "3Dキャラクター",
            "Authored専用テクスチャ",
            &["VRChat"],
            "Texture",
        ),
        (
            "3Dキャラクター",
            "オリジナル3Dモデル「Authored」",
            &["VRChat", "テクスチャ"],
            "Base",
        ),
        (
            "3Dキャラクター",
            "オリジナル3Dモデル「Authored」+追加テクスチャ",
            &["VRChat"],
            "Base",
        ),
        (
            "3Dキャラクター",
            "Authored Model with Textures",
            &["VRChat"],
            "Base",
        ),
        (
            "3Dキャラクター",
            "Authored専用テクスチャセット",
            &["VRChat"],
            "Texture",
        ),
        (
            "3Dキャラクター",
            "Authored Model",
            &["チクワカーシステム", "VRChat"],
            "Base",
        ),
        (
            "3Dキャラクター",
            "【販売ワールド】Authored",
            &["VRChat"],
            "Base",
        ),
        (
            "3Dモデル（その他）",
            "Authored Pin",
            &["VRChat", "ワールド固定", "小物"],
            "Accessory",
        ),
        (
            "3D環境・ワールド",
            "ギミック付き Authored Room",
            &["VRChat", "Udon"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Room ギミック有り",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Room（Udonギミック付き）",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Udon対応 Authored Room",
            &["Udon"],
            "World",
        ),
        ("3D環境・ワールド", "モダン Authored スツール", &[], "World"),
        (
            "3D環境・ワールド",
            "Authored ツールームアパート",
            &[],
            "World",
        ),
        ("3D環境・ワールド", "Room【設定ツール同梱】", &[], "World"),
        (
            "3D環境・ワールド",
            "Room Udon ギミック付き",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Room シェーダー同梱",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Room with Gimmicks",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored Solar System",
            &["VRChat"],
            "World",
        ),
        (
            "3D環境・ワールド",
            "Authored 雨粒シェーダー",
            &["VRChat"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "AuthoredWaterShader",
            &["VRChat"],
            "Shader",
        ),
        (
            "3D環境・ワールド",
            "【VRC Udonギミック】Authored Darts",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Slot【UdonChips】",
            &["VRChat"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "【VRChatワールドギミック】Authored Camera",
            &["VRChat", "Udon"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "[VRCワールド] AUTHORED CAR SYSTEM",
            &["udon"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "Authored Lift for VRChat Worlds",
            &["Udon"],
            "Udon",
        ),
        (
            "3D環境・ワールド",
            "ワールド制作支援ツール",
            &["VRChat"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored 影シェーダー【導入ツール同梱】",
            &["VRChat"],
            "Shader",
        ),
        (
            "3Dツール・システム",
            "シェーダー変換ツール",
            &["VRChat"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored 音ギミック",
            &["VRChat", "シェーダー"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "AuthoredLens",
            &["VRChat", "VRChatギミック"],
            "Gimmick",
        ),
        (
            "3Dツール・システム",
            "AuthoredMat",
            &["VRChat", "シェーダー", "VRChatギミック"],
            "Shader",
        ),
        (
            "3Dツール・システム",
            "AuthoredBed",
            &["VRChat", "Udon", "ギミック"],
            "Udon",
        ),
        (
            "3Dツール・システム",
            "AuthoredTool",
            &["VRChat", "ツール", "ギミック"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "AuthoredKit",
            &["VRChat", "エディタ拡張", "ギミック"],
            "Tool",
        ),
        (
            "3Dツール・システム",
            "Authored Fitter",
            &["VRChat", "衣装", "テクスチャ"],
            "Tool",
        ),
        (
            "3Dモーション・アニメーション",
            "Authored AFK",
            &["VRChat", "衣装", "シェーダー"],
            "Motion",
        ),
        (
            "3Dテクスチャ",
            "Authored Eye",
            &["VRChat", "モーション", "シェーダー"],
            "Texture",
        ),
        ("3Dテクスチャ", "Authored Eye", &[], "Texture"),
        (
            "3Dテクスチャ",
            "AuthoredShader専用マテリアル",
            &["VRChat", "シェーダー"],
            "Texture",
        ),
        (
            "3Dテクスチャ",
            "Authored Skin シェーダー",
            &["VRChat"],
            "Shader",
        ),
        (
            "3Dテクスチャ",
            "Authored Frost Glass Shader",
            &["VRChat", "Shader"],
            "Shader",
        ),
        (
            "3Dテクスチャ",
            "Authored シェーダーに使用できるマスクテクスチャ",
            &["VRChat"],
            "Texture",
        ),
        (
            "3Dテクスチャ",
            "Authored Eye",
            &["VRChat", "Shader"],
            "Texture",
        ),
        (
            "3Dテクスチャ",
            "Authored Eye Add-on",
            &["ソフトウェア"],
            "Texture",
        ),
        ("3D衣装", "Authored Dress Add-on", &[], "Clothes"),
        (
            "3D衣装",
            "Authored Dress",
            &["VRChat", "ソフトウェア"],
            "Clothes",
        ),
        ("3D衣装", "着せ替え エディタ拡張", &["VRChat"], "Tool"),
        (
            "3D靴",
            "Authored Boots",
            &["VRChat", "シェーダー"],
            "Clothes",
        ),
        ("3D髪型", "Authored Bob", &["VRChat", "衣装"], "Hair"),
        (
            "3D装飾品",
            "Authored Ring",
            &["VRChat", "テクスチャ"],
            "Accessory",
        ),
        (
            "3D小道具",
            "Authored Cup",
            &["VRChat", "ワールド"],
            "Accessory",
        ),
        // A generic category leaves the type to the tags.
        (
            "3Dモデル（その他）",
            "Authored Pin",
            &["VRChat", "アクセサリー"],
            "Accessory",
        ),
        (
            "3Dモデル（その他）",
            "【販売ワールド】Authored Room",
            &["VRChat", "Udon"],
            "World",
        ),
        (
            "3Dモデル（その他）",
            "Authored Cape",
            &["VRChat", "世界観衣装"],
            "Clothes",
        ),
        (
            "3Dモデル（その他）",
            "Authored プロポーズリング",
            &["VRChat"],
            "NonVRChat",
        ),
        // It is 3D data already, so these need no VRChat or Unity word.
        (
            "3Dモデル（その他）",
            "Authored Walk",
            &["アニメーション"],
            "Motion",
        ),
        (
            "3Dモデル（その他）",
            "Authored World 紹介動画あり",
            &[],
            "World",
        ),
    ] {
        let source = booth_listing("3Dモデル", leaf, title, tags);
        assert_eq!(
            infer_product_category(&source)["category"],
            expected,
            "{leaf} {title}"
        );
    }
    // Declared software in tools and motion categories still decides, as for Base.
    assert_eq!(
        infer_product_category(&booth_listing(
            "3Dモデル",
            "3Dモーション・アニメーション",
            "Blenderアドオン Authored",
            &["Blender", "Unity"]
        ))["category"],
        "Tool"
    );
    // A tool outside any VRChat or Unity context stays out of the default list.
    let result = infer_product_category(&booth_listing(
        "3Dモデル",
        "3Dツール・システム",
        "Authored Blender Tool",
        &["Blender"],
    ));
    assert_eq!(result["category"], "NonVRChat");
    assert_eq!(result["scope"], "unknown");
    // The store's own 3D category stands against a tag for what its sources need.
    for (leaf, title, tags, expected, scope) in [
        (
            "3D衣装",
            "VRChat向け Authored Dress",
            &["Photoshop"][..],
            "Clothes",
            "vrchat",
        ),
        (
            "3D衣装",
            "Authored Dress",
            &["Photoshop", "Windows"],
            "Clothes",
            "unknown",
        ),
        (
            "3Dテクスチャ",
            "Authored Eye",
            &["Photoshop"],
            "Texture",
            "unknown",
        ),
        (
            "3D環境・ワールド",
            "Authored Room",
            &["Windows"],
            "World",
            "unknown",
        ),
        (
            "3Dキャラクター",
            "VRChat向け Authored Model",
            &["Photoshop"],
            "Base",
            "vrchat",
        ),
        // A title naming the application is not about source files.
        (
            "3Dモーション・アニメーション",
            "After Effects用 Authored Preset",
            &[],
            "Motion",
            "other",
        ),
        // A tool has to show where it runs, whatever a title separates it into.
        (
            "3Dツール・システム",
            "Authored Tool",
            &["Unity", "AE"],
            "NonVRChat",
            "other",
        ),
        (
            "3Dツール・システム",
            "Authored Blender Shader",
            &["Blender"],
            "NonVRChat",
            "unknown",
        ),
        (
            "3Dツール・システム",
            "Authored Glow シェーダー",
            &["AfterEffects"],
            "NonVRChat",
            "other",
        ),
    ] {
        let result = infer_product_category(&booth_listing("3Dモデル", leaf, title, tags));
        assert_eq!(result["category"], expected, "{leaf} {title}");
        assert_eq!(result["scope"], scope, "{leaf} {title}");
    }
    // A description calling a garment an add-on is not defining software.
    let swimsuit = infer_product_category(
        &json!({"store":"BOOTH","sourceCategory":{"name":"3D衣装","parentName":"3Dモデル"},"sourceTitle":"Authored","sourceDescription":"Authored用の水着アドオンです。"}),
    );
    assert_eq!(swimsuit["category"], "Clothes");
    // Evidence is what decided, and a listing with no category has none to cite.
    let titled =
        infer_product_category(&json!({"store":"BOOTH","sourceTitle":"【販売ワールド】Authored"}));
    assert_eq!(titled["category"], "World");
    assert_eq!(
        titled["evidence"],
        json!([{"field":"sourceTitle","value":"【販売ワールド】Authored"}])
    );
    let shader = infer_product_category(&booth_listing(
        "3Dモデル",
        "3D環境・ワールド",
        "Authored シェーダー",
        &["VRChat"],
    ));
    assert_eq!(
        shader["evidence"],
        json!([{"field":"sourceTitle","value":"Authored シェーダー"}])
    );
    let world = infer_product_category(&booth_listing(
        "3Dモデル",
        "3D環境・ワールド",
        "Authored",
        &["Udon"],
    ));
    assert_eq!(
        world["evidence"],
        json!([{"field":"sourceCategory.name","value":"3D環境・ワールド"}])
    );
    // A decision from a tag cites the tag that made it.
    for (leaf, tags, tag, kind) in [
        (
            "3D環境・ワールド",
            vec!["VRChat", "ワールドギミック"],
            "ワールドギミック",
            "Udon",
        ),
        (
            "3D環境・ワールド",
            vec!["VRChat", "チクワカーシステム"],
            "チクワカーシステム",
            "Udon",
        ),
        (
            "ソフトウェア",
            vec!["VRChat", "ModularAvatar対応"],
            "ModularAvatar対応",
            "Gimmick",
        ),
        ("ソフトウェア", vec!["VRChat", "U#"], "U#", "Udon"),
    ] {
        let result = infer_product_category(&booth_listing("3Dモデル", leaf, "Authored", &tags));
        assert_eq!(result["category"], kind, "{leaf}");
        assert_eq!(result["basis"], "sourceTags", "{leaf}");
        assert_eq!(
            result["evidence"],
            json!([{"field":"sourceTags","value":tag}]),
            "{leaf}"
        );
    }
}

#[test]
fn stores_other_than_booth_keep_their_rules() {
    for (source, expected) in [
        (
            json!({"provider":"gumroad","sourceTitle":"Authored","sourceDescription":"VRChat outfit for Authored."}),
            "Clothes",
        ),
        (
            json!({"provider":"jinxxy","sourceTitle":"Authored","sourceDescription":"A cute outfit for Authored."}),
            "Clothes",
        ),
        (
            json!({"provider":"jinxxy","sourceTitle":"Authored World VFX"}),
            "World",
        ),
        (
            json!({"provider":"local","sourceTitle":"Authored Walk Animation"}),
            "Motion",
        ),
        (
            json!({"provider":"gumroad","sourceCategory":{"name":"3D環境・ワールド"},"sourceTitle":"VRChat Authored","sourceTags":["Udon"]}),
            "Udon",
        ),
        (
            json!({"provider":"gumroad","sourceCategory":{"name":"3D環境・ワールド"},"sourceTitle":"Authored"}),
            "NonVRChat",
        ),
        (
            json!({"provider":"gumroad","sourceTitle":"VRChat Authored","sourceTags":["ワールドギミック"]}),
            "Gimmick",
        ),
        (
            json!({"provider":"jinxxy","sourceTitle":"[CVS2] Authored"}),
            "Gimmick",
        ),
    ] {
        assert_eq!(
            infer_product_category(&source)["category"],
            expected,
            "{source}"
        );
    }
}

#[test]
fn tags_outside_booth_3d_categories_need_vrchat_or_unity_evidence() {
    for (parent, leaf, title, tags, expected) in [
        (
            "素材データ",
            "背景画像",
            "【動く背景】Authored Room",
            &["アニメーション", "配信背景"][..],
            "NonVRChat",
        ),
        (
            "映像作品",
            "アニメーション",
            "Authored 第1話 DVD",
            &["アニメーション"],
            "NonVRChat",
        ),
        (
            "映像作品",
            "アニメーション",
            "Authored Blu-ray",
            &[],
            "NonVRChat",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored Loop",
            &["アニメーション", "モーショングラフィックス", "VJ素材"],
            "NonVRChat",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored プリセット",
            &["モーション", "プリセット"],
            "NonVRChat",
        ),
        (
            "イラスト",
            "メイキング・講座",
            "Authored 講座",
            &["アニメーション"],
            "NonVRChat",
        ),
        (
            "ファッション",
            "パーカー",
            "Authored Hoodie",
            &["衣装"],
            "NonVRChat",
        ),
        (
            "アクセサリー",
            "ペンダント",
            "Authored Pendant",
            &[],
            "NonVRChat",
        ),
        (
            "素材データ",
            "BGM素材",
            "Authored BGM",
            &["異世界"],
            "NonVRChat",
        ),
        // The motion data sellers file there keeps its place.
        (
            "素材データ",
            "素材（その他）",
            "Authored Idle",
            &["VRChat", "アニメーション", "モーション素材"],
            "Motion",
        ),
        (
            "映像作品",
            "アニメーション",
            "VRC用 Authored Motion",
            &["VRChat", "Emote"],
            "Motion",
        ),
        (
            "映像作品",
            "アニメーション",
            "Authored Animation Pack",
            &["VRChat"],
            "Motion",
        ),
        (
            "映像作品",
            "アニメーション",
            "Authored 表情アニメーション",
            &["Unity", "アニメーション"],
            "Motion",
        ),
        (
            "ゲーム",
            "ゲーム関連商品",
            "Authored Combo",
            &["VRChat", "アニメーション", "ツール"],
            "Motion",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored Idle",
            &["VRChat", "エモート"],
            "Motion",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored Set",
            &["VRChat", "ポーズ集"],
            "Motion",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored ポーズ集",
            &["イラスト用"],
            "NonVRChat",
        ),
        // Particle sellers tag アニメーション and every platform; none of that is the type.
        (
            "素材データ",
            "素材（その他）",
            "【Unity/VRChat】Authored Fire",
            &["アニメーション", "Unity", "パーティクル", "VRChat"],
            "NonVRChat",
        ),
        (
            "映像作品",
            "アニメーション",
            "【Unity/VRChat】World of Authored",
            &["アニメーション", "Unity", "エフェクト", "vrchat world"],
            "NonVRChat",
        ),
        (
            "映像作品",
            "アニメーション",
            "【VRChatライブ映像】Authored",
            &["VRChat"],
            "NonVRChat",
        ),
        // Other types still come from their tags once the scope is explicit.
        (
            "ゲーム",
            "ゲーム関連商品",
            "Authored Lift script for VRChat Worlds",
            &["VRChat", "Udon"],
            "Udon",
        ),
        (
            "ゲーム",
            "ゲーム関連商品",
            "【販売ワールド】Authored Room",
            &["VRChat", "Udon"],
            "World",
        ),
        (
            "素材データ",
            "素材（その他）",
            "Authored Water",
            &["Unity", "シェーダー"],
            "Shader",
        ),
        (
            "素材データ",
            "背景画像",
            "Authored Skybox",
            &["VRChat", "ワールド"],
            "World",
        ),
        (
            "素材データ",
            "背景画像",
            "Authored Eye",
            &["VRChat", "テクスチャ"],
            "Texture",
        ),
    ] {
        let source = booth_listing(parent, leaf, title, tags);
        assert_eq!(
            infer_product_category(&source)["category"],
            expected,
            "{leaf} {title}"
        );
    }
    // A VRChat video is no asset type, and its VRChat word keeps it in the default list.
    let video = infer_product_category(&booth_listing(
        "映像作品",
        "アニメーション",
        "【VRChatライブ映像】Authored",
        &["VRChat"],
    ));
    assert_eq!(video["category"], "NonVRChat");
    assert_eq!(video["scope"], "vrchat");
}

#[test]
fn a_removed_listing_title_names_motion_only_with_motion_words() {
    for (title, expected) in [
        ("Authored 待機モーション", "Motion"),
        ("Authored Motion Pack", "Motion"),
        ("Authored アニメーション", "NonVRChat"),
        ("VRChat用 Authored アニメーション", "Motion"),
        ("Authored モーショングラフィックス", "NonVRChat"),
        ("Authored 動作確認済み", "NonVRChat"),
        ("Authored Twitch Emote Pack", "NonVRChat"),
        ("Authored ポーズ集", "NonVRChat"),
        ("VRChat用 Authored ポーズ集", "Motion"),
    ] {
        assert_eq!(
            infer_product_category(
                &json!({"store":"BOOTH","sourceTitle":title,"publicPageAvailability":"unavailable"})
            )["category"],
            expected,
            "{title}"
        );
    }
}

#[test]
fn a_description_names_a_type_only_by_defining_it() {
    for description in [
        "テクスチャと衣装を含みます。ワールドでも使えます。",
        "VRChat 向けの小物です。ヘアゴムとアクセサリーがセットです。",
        "Includes a texture, a tool and a world prefab.",
    ] {
        let result = infer_product_category(
            &json!({"store":"BOOTH","sourceCategory":{"name":"3Dモデル（その他）","parentName":"3Dモデル"},"sourceTitle":"Authored","sourceTags":["VRChat"],"sourceDescription":description}),
        );
        assert_eq!(result["category"], "NonVRChat", "{description}");
        // It stays in the default list as Unclassified, because it says VRChat.
        assert_eq!(result["scope"], "vrchat", "{description}");
    }
    for (description, expected) in [
        ("Unity shader for skin.", "Shader"),
        ("Unity editor tool for avatars.", "Tool"),
    ] {
        assert_eq!(
            infer_product_category(
                &json!({"store":"BOOTH","sourceCategory":{"name":"3Dモデル（その他）","parentName":"3Dモデル"},"sourceTitle":"Authored","sourceTags":["VRChat"],"sourceDescription":description})
            )["category"],
            expected,
            "{description}"
        );
    }
}

#[test]
fn product_scope_and_description_evidence_are_bounded() {
    let base = json!({"sourceTitle":"Texture Tool","sourceTags":["Windows","AE"],"description":"Exports textures usable in VRChat and Unity"});
    assert_eq!(infer_product_category(&base)["category"], "NonVRChat");
    assert_eq!(infer_product_category(&base)["scope"], "other");
    let mut explicit = base.clone();
    explicit["tags"] = json!(["VRChat"]);
    assert_eq!(infer_product_category(&explicit)["category"], "Texture");
    assert_eq!(infer_product_category(&explicit)["scope"], "vrchat");
    assert_eq!(
        infer_product_category(
            &json!({"sourceTitle":"Editor Tool","description":"Compare Unity alternatives"})
        )["category"],
        "NonVRChat"
    );
    assert_eq!(
        infer_product_category(
            &json!({"sourceTitle":"Editor Tool","description":"Import this Unity editor extension"})
        )["scope"],
        "unity"
    );
    assert_eq!(
        infer_product_category(
            &json!({"sourceTitle":"Editor Tool","sourceTags":["Windows","Unity"]})
        )["category"],
        "Tool"
    );
    assert_ne!(
        infer_product_category(
            &json!({"sourceTitle":"New item","sourceTags":["VRChat"],"description":"Requires lilToon shader and Unity."})
        )["category"],
        "Shader"
    );
    assert_eq!(
        infer_product_category(
            &json!({"sourceTitle":"Dress","sourceTags":["VRChat","衣装"],"description":"Unity shader package required: lilToon"})
        )["category"],
        "Clothes"
    );
    let fallback = infer_product_category(
        &json!({"sourceDescription":"","description":format!("Unity shader uGUI {}", "x".repeat(40_000))}),
    );
    assert_eq!(fallback["category"], "Shader");
    assert_eq!(fallback["scope"], "unity");
    assert!(fallback.to_string().len() < 1500);
}

#[test]
fn external_opening_context_and_animation_dependencies_do_not_define_vr_products() {
    for app in [
        "After Effects",
        "AE",
        "NovelAI",
        "CoC",
        "Live2D",
        "ココフォリア",
    ] {
        let title_context = json!({"sourceTitle":format!("{app} Tool"),"sourceTags":["ツール"],"description":"Results may be used in VRChat"});
        let description_context = json!({"sourceTitle":"Texture Pack","sourceTags":["Texture"],"sourceDescription":format!("Materials for {app}. Some results can also be used in VRChat.")});
        for mut source in [title_context, description_context] {
            let result = infer_product_category(&source);
            assert_eq!(result["category"], "NonVRChat", "{app}");
            assert_eq!(result["scope"], "other", "{app}");
            source["tags"] = json!(["VRChat"]);
            let explicit = infer_product_category(&source);
            assert_ne!(explicit["category"], "NonVRChat", "explicit tag {app}");
            assert_eq!(explicit["scope"], "vrchat", "explicit tag {app}");
        }
    }
    let windows = infer_product_category(
        &json!({"sourceTitle":"Windows Unity Tool","sourceTags":["Windows","Unity"]}),
    );
    assert_eq!(windows["category"], "Tool");
    assert_eq!(windows["scope"], "unity");
    let late_mention = infer_product_category(
        &json!({"sourceTitle":"Unity Tool","sourceTags":["Unity"],"description":format!("{} Photoshop is used in an optional example.", "x".repeat(250))}),
    );
    assert_eq!(late_mention["category"], "Tool");
    assert_eq!(late_mention["scope"], "unity");
    assert!(!strong_external_application("aesthetic tool coconut"));
    let mut motion = json!({"sourceTitle":"Ambient effect","sourceTags":["Unity"],"description":"Requires an animation controller."});
    assert_eq!(infer_product_category(&motion)["category"], "NonVRChat");
    motion["sourceTags"] = json!(["Unity", "Motion"]);
    assert_eq!(infer_product_category(&motion)["category"], "Motion");
    motion["sourceTags"] = json!(["Unity"]);
    motion["sourceTitle"] = json!("Humanoid Motion");
    assert_eq!(infer_product_category(&motion)["category"], "Motion");
}

#[test]
fn windows_tool_requires_explicit_vr_tag_or_meaningful_unity_context() {
    let mut source = json!({"sourceTitle":"Windows Tool","sourceTags":["Windows","Tools"],"description":"Its output may also be used in VRChat."});
    let unrelated = infer_product_category(&source);
    assert_eq!(unrelated["category"], "NonVRChat");
    assert_eq!(unrelated["scope"], "other");
    source["tags"] = json!(["VRChat"]);
    let explicit = infer_product_category(&source);
    assert_eq!(explicit["category"], "Tool");
    assert_eq!(explicit["scope"], "vrchat");
    source["tags"] = json!([]);
    source["description"] = json!("Import this Unity editor extension.");
    let unity = infer_product_category(&source);
    assert_eq!(unity["category"], "Tool");
    assert_eq!(unity["scope"], "unity");
}

#[test]
fn avatar_labels_clean_only_exact_names_and_keep_unknown_identity() {
    for text in [
        "オリジナル3Dモデル「真冬」Ver.1.02",
        "真冬 Mafuyu / オリジナル3Dモデル",
        "真冬 / Mafuyu",
        "【オリジナル3Dモデル】真冬",
    ] {
        assert_eq!(clean_avatar_name(text), "真冬");
        assert_eq!(canonical_tag(text), "booth:5007531");
    }
    assert_eq!(
        clean_avatar_name("シフォン -Chiffon- / オリジナル3Dモデル"),
        "シフォン"
    );
    assert_ne!(canonical_tag("真冬用コート"), "booth:5007531");
    assert_ne!(canonical_tag("Mafuyu / coat"), "booth:5007531");
    assert_ne!(canonical_tag("真冬 / マヌカ"), "booth:5007531");
    assert_eq!(romanized_name("シロネコ"), "Shironeko");
    assert_eq!(romanized_name("きゃっと"), "Kyatto");
    assert_eq!(romanize_kana("未確認漢字"), "");
    assert_eq!(romanize_kana("未確認漢字ねこ"), "");
    #[cfg(target_os = "windows")]
    assert_eq!(romanized_name("星空猫"), "Hoshizoraneko");
    #[cfg(not(target_os = "windows"))]
    assert_eq!(romanized_name("未確認漢字"), "");
    assert_eq!(
        avatar_display_name(&json!({"officialName":"Example","englishName":"Example"})),
        "Example"
    );
    let options = owned_base_options(&[
        json!({"id":"unknown-id","category":"Base","sourceTitle":"オリジナル3Dモデル「真冬」"}),
    ]);
    assert_eq!(options[0]["id"], "product:unknown-id");
    assert_eq!(options[0]["displayName"], "真冬 / Mafuyu");
    assert_eq!(options[0]["englishNameStatus"], "romanizationFallback");
    assert_eq!(options[0]["entitlementEstablished"], false);
    let mut statuses = BTreeMap::new();
    for avatar in avatars() {
        assert!(!avatar.value["englishName"].as_str().unwrap().is_empty());
        *statuses
            .entry(avatar.value["englishNameStatus"].as_str().unwrap())
            .or_insert(0) += 1;
    }
    assert_eq!(statuses["official"], 28);
    assert_eq!(statuses["romanizationFallback"], 3);
    assert_eq!(statuses["userAccepted"], 1);
}

#[test]
fn base_supplement_uses_identity_not_similar_title() {
    let options = owned_base_options(&[
        json!({"id":"5007531","category":"Base","title":"custom rename"}),
        json!({"id":"5007531","category":"Base","title":"same ID"}),
        json!({"id":"synthetic-other","category":"Base","sourceTitle":"真冬","title":"renamed"}),
        json!({"id":"synthetic-clothes","category":"Clothes","title":"真冬"}),
    ]);
    assert_eq!(options.len(), 2);
    assert_eq!(options[0]["officialName"], "真冬");
    assert_eq!(options[1]["id"], "product:synthetic-other");
    assert!(options
        .iter()
        .all(|option| option["entitlementEstablished"] == false));
}

#[test]
fn every_effective_base_keeps_its_identity_even_when_label_is_uncertain() {
    let products = [
        json!({"id":"authored-wrapper","category":"Base","sourceTitle":"Original 3D Avatar"}),
        json!({"id":"authored-ambiguous","category":"Base","sourceTitle":"星猫 / 海猫"}),
        json!({"id":"authored-bundle","category":"Base","sourceTitle":"【 R18 Edition】苺佳(ichica) and GLC and 蒼葉(aoba)"}),
        json!({"id":"authored-latin","category":"Base","sourceTitle":"【無料】Ren♡Ai"}),
        json!({"id":"authored-unsupported","category":"Base","sourceTitle":"そらΩ"}),
        json!({"id":"authored-empty","category":"Base","sourceTitle":"","title":""}),
        json!({"id":"authored-other","category":"Clothes","sourceTitle":"アマト"}),
    ];
    let options = owned_base_options(&products);
    assert_eq!(options.len(), 6);
    for product in &products[..6] {
        assert!(options
            .iter()
            .any(|option| option["id"] == format!("product:{}", product["id"].as_str().unwrap())));
    }
    for id in ["authored-wrapper", "authored-ambiguous", "authored-bundle"] {
        let option = options
            .iter()
            .find(|option| option["productId"] == id)
            .unwrap();
        assert!(!option["displayName"].as_str().unwrap().is_empty());
        assert_eq!(option["englishName"], "");
        assert_ne!(option["nameKind"], "single");
    }
    let latin = options
        .iter()
        .find(|option| option["productId"] == "authored-latin")
        .unwrap();
    assert_eq!(latin["displayName"], "Ren♡Ai");
    assert_eq!(latin["englishNameStatus"], "sourceTitleSpelling");
    assert!(options
        .iter()
        .all(|option| option["entitlementEstablished"] == false));
}

#[test]
fn public_identity_presentation_corrections_never_spread_to_similar_products() {
    for (id, source, expected) in [
        (
            "8547624",
            "銀髪スーツの悪魔 メイネーラ VRChat向けオリジナル3Dアバター #e62f8b",
            "メイネーラ",
        ),
        (
            "4309253",
            "ブルーアーカイブ 二次創作3Dアバター 棗 イロハ",
            "棗 イロハ",
        ),
    ] {
        let options =
            owned_base_options(&[json!({"id":id,"category":"Base","sourceTitle":source})]);
        assert_eq!(options[0]["id"], format!("product:{id}"));
        assert_eq!(options[0]["officialName"], expected);
        assert!(matches!(
            options[0]["englishNameStatus"].as_str(),
            Some("romanizationFallback" | "unresolvedReading")
        ));
        assert_eq!(
            options[0]["englishNameEvidence"]["kind"],
            "presentationFallback"
        );
        assert_ne!(
            parsed_product_avatar_label("authored-other", source).original,
            expected
        );
    }
    assert_eq!(
        parsed_product_avatar_label("8547624", "Authored other name").original,
        "Authored other name"
    );
    let label = owned_base_options(&[
        json!({"id":"authored-nanase","category":"Base","sourceTitle":"【VRChat想定 オリジナル3Dモデル『ナナセ・ノワール -Nanase Noir-』Gen3.0"}),
    ]);
    assert_eq!(
        label[0]["displayName"],
        "ナナセ・ノワール / Nanase Noir (Gen3.0)"
    );
}

#[test]
fn bundled_identities_are_unique_and_have_public_provenance() {
    let mut ids = BTreeSet::new();
    for avatar in avatars() {
        assert!(ids.insert(avatar.value["id"].as_str().unwrap()));
        assert!(avatar.value["productId"]
            .as_str()
            .unwrap()
            .chars()
            .all(|c| c.is_ascii_digit()));
        let url = avatar.value["sourceUrl"].as_str().unwrap();
        assert!(url.starts_with("https://") && url.contains("booth.pm/") && !url.contains('?'));
        assert!(!avatar.names.is_empty());
    }
}

#[test]
fn independently_nominated_baseline_remains_bounded() {
    let evaluation: Value = serde_json::from_str(include_str!("evaluation.json")).unwrap();
    let rows = evaluation["identities"].as_array().unwrap();
    let matched = rows
        .iter()
        .filter(|row| {
            match_avatars(row["name"].as_str().unwrap())
                .iter()
                .any(|candidate| candidate["productId"] == row["productId"])
        })
        .count();
    assert_eq!(rows.len(), 10);
    assert_eq!(matched, 8);
    assert!(dictionary()["coverage"]["populationCoverage"].is_null());
}
