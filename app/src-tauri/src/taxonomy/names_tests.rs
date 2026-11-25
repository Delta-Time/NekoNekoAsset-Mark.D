use super::*;

#[test]
fn supplied_base_title_examples_keep_names_source_spellings_and_editions() {
    let cases = [
        (
            "【VRChat想定 オリジナル3Dモデル『ナナセ・ノワール -Nanase Noir-』Gen3.0",
            "ナナセ・ノワール",
            Some("Nanase Noir"),
            "Gen3.0",
        ),
        ("二次創作3Dモデル「一姫」&#x20;", "一姫", None, ""),
        ("アマト【lilToon対応】", "アマト", None, ""),
        ("【無料】Ren♡Ai", "Ren♡Ai", Some("Ren♡Ai"), ""),
        (
            "【VRChat向けオリジナル3Dモデル】星凪 ゆらぎ",
            "星凪 ゆらぎ",
            None,
            "",
        ),
        ("【Quest対応】寿司ザウルス", "寿司ザウルス", None, ""),
        (
            "オリジナル 3Dモデル『ひかるん - Hikarun -』&#x20;",
            "ひかるん",
            Some("Hikarun"),
            "",
        ),
        (
            "🐲 オリジナル 3Dモデル『リュミィ - Ryumii -』&#x20;",
            "リュミィ",
            Some("Ryumii"),
            "",
        ),
        (
            "銀髪スーツの悪魔 メイネーラ VRChat向けオリジナル3Dアバター #e62f8b",
            "銀髪スーツの悪魔 メイネーラ",
            None,
            "",
        ),
        (
            "「stier」-シュティア-#シュティア3D",
            "シュティア",
            Some("stier"),
            "",
        ),
        (
            "『たゆた -Tayuta-』オリジナル3Dモデル",
            "たゆた",
            Some("Tayuta"),
            "",
        ),
        (
            "[ENG][TORIBASE][GAMEREADY] ERP FOXIE",
            "ERP FOXIE",
            Some("ERP FOXIE"),
            "",
        ),
        (
            "【無料】GLC~ゴスもどきちゃん~Medium",
            "GLC~ゴスもどきちゃん~",
            None,
            "Medium",
        ),
        (
            "【無料配布】オリジナル3Dモデル「フェリス」",
            "フェリス",
            None,
            "",
        ),
        (
            "島風 (Shimakaze) (blend file)&#x20;",
            "島風",
            Some("Shimakaze"),
            "",
        ),
        (
            "【デルタフレア】オリジナル3Dモデル",
            "デルタフレア",
            None,
            "",
        ),
        ("ホノカ - Honoka #Honoka3D", "ホノカ", Some("Honoka"), ""),
        (
            "「ネミア -Nenmir-」 #MARUBODY&#x20;",
            "ネミア",
            Some("Nenmir"),
            "",
        ),
        (
            "ブルーアーカイブ 二次創作3Dアバター 棗 イロハ",
            "ブルーアーカイブ 二次創作3Dアバター 棗 イロハ",
            None,
            "",
        ),
        (
            "\"つみげ -Tsumige-\"【VRChat向けオリジナルアバター】&#x20;",
            "つみげ",
            Some("Tsumige"),
            "",
        ),
        (
            "『だいふくおばけ』 VRC想定オリジナル3Dアバター",
            "だいふくおばけ",
            None,
            "",
        ),
    ];
    let mut failures = Vec::new();
    for (title, original, latin, edition) in cases {
        let parsed = structured_base_parts(title);
        let qualifiers = if edition.is_empty() {
            vec![]
        } else {
            vec![edition.to_owned()]
        };
        if parsed.original != original
            || parsed.source_latin.as_deref() != latin
            || parsed.name_kind != "single"
            || parsed.qualifiers != qualifiers
        {
            failures.push(format!("{title}: {parsed:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    let bundle = structured_base_parts("【 R18 Edition】苺佳(ichica) and GLC and 蒼葉(aoba)");
    assert_eq!(bundle.original, "苺佳(ichica) and GLC and 蒼葉(aoba)");
    assert_eq!(bundle.source_latin, None);
    assert_eq!(bundle.name_kind, "bundle");
    assert_eq!(bundle.qualifiers, ["R18 Edition"]);
}

#[test]
fn product_titles_locate_their_name_through_unlisted_marketing_text() {
    let cases: &[(&str, &str, Option<&str>, &[&str])] = &[
        // Status and sale wording needs no vocabulary entry.
        (
            "【発売記念SALE✧】ソラネコ -Soraneko- / オリジナル3Dモデル",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【オリジナル3Dモデル】soraneko-ソラネコ 【販売終了】",
            "ソラネコ",
            Some("soraneko"),
            &[],
        ),
        (
            "【オリジナル３Ｄモデル】ソラネコ・ノワール【個人商用ライセンス版】",
            "ソラネコ・ノワール",
            None,
            &[],
        ),
        (
            "【サポート終了】【version2.5】ソラネコ【オリジナル3Dモデル】",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "【オリジナル3Dモデル】ソラネコ (2030年1月31日まで販売)",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "🎊セール中！ 12/24まで🎊【VRChat向けオリジナル3Dモデル】星空 ソラネコ",
            "星空 ソラネコ",
            None,
            &[],
        ),
        (
            "★R18★ オリジナル3Ｄモデル 【ソラネコ】",
            "ソラネコ",
            None,
            &[],
        ),
        ("[DX版]ソラネコ (VRChat向け3Dモデル)", "ソラネコ", None, &[]),
        // A quote outranks all text around it, malformed or not.
        (
            "SALE中✨オリジナル3Dモデル『ソラネコ』☕3000→1500",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "「ソラ&ネコ」キャラクター素体 v3.2 【VRChat Blender】",
            "ソラ&ネコ",
            None,
            &[],
        ),
        (
            "オリジナル3Dモデル『ソラネコ』Avatar3.0向け",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "VRChat向けオリジナル3Dモデル「Soraneko」Mobile対応",
            "Soraneko",
            Some("Soraneko"),
            &[],
        ),
        (
            "VRC向け3Dモデル「Soraneko - ソラネコ -」 +VRM付き PB対応",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "Quest・PC対応『ソラネコ -Soraneko-』オリジナル3Dモデル",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【無料版あり】オリジナル3Dモデル「ソラネコ」【VRC / VRM / MMD】",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "オリジナル3Dモデル『ソラネコ -Soraneko- 』 #ソラネコ3D #Soraneko3D / AC9",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "オリジナル3Dモデル『ソラネコ - Soraneko』💎✨️",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【オリジナル3Dモデル】星空の主『ソラネコ』",
            "ソラネコ",
            None,
            &[],
        ),
        ("ブランドオリジナルアバター “K”", "K", Some("K"), &[]),
        ("オリジナル3Dモデル〚ソラネコ〛", "ソラネコ", None, &[]),
        // A spelling can sit on either side, across any separator or bracket.
        (
            "【軽量版無料】オリジナル3Dモデル｢ソラネコ｣ -Soraneko-",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【3Dモデル】『ソラネコ ver.1.3』Soraneko",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "オリジナル３Dモデル 【ソラネコ】 - Soraneko",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "オリジナル3Dモデル 『Soraneko -ソラネコ-』",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【オリジナル3Dモデル】Soraneko - ソラネコ",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "-ソラネコ- Soraneko【オリジナル3Dモデル】",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【オリジナル3Dモデル】Soraneko -ソラネコ-【AuthoredBody】",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "オリジナル3Dモデル【ソラネコ:Soraneko】",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "オリジナル3Dモデル「ソラネコ・Soraneko」",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        ("オリジナル3Dモデル『ソラ -So-』", "ソラ", Some("So"), &[]),
        (
            "【オリジナル3Dモデル】Re:ソラネコ",
            "Re:ソラネコ",
            None,
            &[],
        ),
        (
            "【オリジナル3Dモデル】ソラネコ[soraneko]",
            "ソラネコ",
            Some("soraneko"),
            &[],
        ),
        (
            "【VRCアバター】 Soraneko (ソラネコ)",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "VRChat想定アバター ソラネコ(Soraneko)",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【-999円】「Soraneko（空猫）」VRChat/VRM",
            "空猫",
            Some("Soraneko"),
            &[],
        ),
        // Text after a name stays out of it.
        (
            "【オリジナル3Dモデル】 ソラネコ （SORANEKO）◆シリーズ共通素体",
            "ソラネコ",
            Some("SORANEKO"),
            &[],
        ),
        (
            "◆オリジナル3Dモデル◆ソラネコ ver1.0 (通常版)",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "オリジナル3Dモデル／ソラネコ (Soraneko) AAA-99-0001",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "【3Dモデル】Soraneko -ソラネコ- + AUTHORED SCARF -スカーフ-【PhysBones対応】",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "ソラネコ+ソラの詰め合わせパック【オリジナル3Dモデル】",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "VRC向け3Dモデル【ソラネコ -Soraneko- 】V2",
            "ソラネコ",
            Some("Soraneko"),
            &[],
        ),
        (
            "２＋２（に↑ぷらす）【オリジナル3Dモデル】",
            "2+2",
            None,
            &[],
        ),
        (
            "ソラネコ オリジナル3Dモデル -VRChatアバター & VRM & 追加衣装",
            "ソラネコ",
            None,
            &[],
        ),
        (
            "「ソラネコ」 オリジナル3Dモデル -VRChatアバター & VRM & 追加衣装",
            "ソラネコ",
            None,
            &[],
        ),
        // Editions are kept beside the name, never as a spelling.
        (
            "オリジナル3Dモデル「ソラネコ」RE",
            "ソラネコ",
            None,
            &["RE"],
        ),
        (
            "【無料】AAA～ソラネコ～Low Polygon edition【オリジナル3Dモデル】",
            "AAA~ソラネコ~",
            None,
            &["Low"],
        ),
        (
            "【無料】AAA～ソラネコ～Medium Polygon edition【オリジナル3Dモデル】",
            "AAA~ソラネコ~",
            None,
            &["Medium"],
        ),
        (
            "【無料】Sora♡Neko / オリジナル3Dモデル【AuthoredVersion】",
            "Sora♡Neko",
            Some("Sora♡Neko"),
            &["AuthoredVersion"],
        ),
        (
            "【VRChat想定 オリジナル3Dモデル『ソラネコ -Soraneko-』Gen3.0",
            "ソラネコ",
            Some("Soraneko"),
            &["Gen3.0"],
        ),
        // Latin variants and series names stay complete and distinct.
        (
            "【3Dモデル】Soraneko -Winter-【PhysBones対応】",
            "Soraneko -Winter-",
            Some("Soraneko -Winter-"),
            &[],
        ),
        (
            "【無料】ソラネコ Builder Edition 2.0【ブロックゲーム、vrchat、vrm対応】",
            "ソラネコ Builder Edition 2.0",
            None,
            &[],
        ),
        (
            "【オリジナル3Dモデル】ウミネコ-UMINEKO-",
            "ウミネコ",
            Some("UMINEKO"),
            &[],
        ),
        (
            "【オリジナル3Dモデル】ソラネコ_NEW",
            "ソラネコ_NEW",
            None,
            &[],
        ),
        (
            "Authored Modeler",
            "Authored Modeler",
            Some("Authored Modeler"),
            &[],
        ),
        ("Umineko【セール中】", "Umineko", Some("Umineko"), &[]),
        // Nothing marks where an epithet ends, so it stays beside the name.
        (
            "星空の悪魔 ソラネコ VRChat向けオリジナル3Dアバター #abc123",
            "星空の悪魔 ソラネコ",
            None,
            &[],
        ),
    ];
    let mut failures = Vec::new();
    for (title, original, latin, qualifiers) in cases {
        let parsed = product_base_parts(title);
        if parsed.original != *original
            || parsed.source_latin.as_deref() != *latin
            || parsed.name_kind != "single"
            || parsed.qualifiers != *qualifiers
        {
            failures.push(format!("{title}: {parsed:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn product_titles_without_one_clear_avatar_name_stay_uncertain() {
    // An add-on keeps its visible label and creates no automatic name.
    for title in [
        "ソラネコ専用【センシティブボディ】",
        "【AUTHORED】ソラネコ&ウミネコ専用テクスチャ",
        "「ソラネコ」キャラクター素体 R18改変支援キット",
        "２＋２（に↑ぷらす）SE拡張【DLC】",
        "「ソラネコ」対応コート",
        "「ソラネコ」 Jacket",
        "【オリジナル3Dモデル】ソラネコ 素体用追加テクスチャ",
        "ソラネコ オリジナル3Dモデル -ウミネコ専用衣装",
        "ソラネコ -VRChatアバター用衣装",
        "「ソラネコ」用R18ボディ v1.1",
        "オリジナル3Dモデル「ソラネコ」用ワンピース",
        "「ソラネコ」向けアバターギミック",
    ] {
        let parsed = product_base_parts(title);
        assert_ne!(parsed.original, "ソラネコ", "{title}");
        assert_eq!(parsed.source_latin, None, "{title}");
        assert_eq!(parsed.name_kind, "fallback", "{title}");
    }
    for title in [
        "オリジナル3Dモデル",
        "VRChat / 3D Avatar",
        "Original 3D Avatar v1.0",
        "星猫 / 海猫",
        "「星猫」「海猫」",
        "【星猫】【海猫】オリジナル3Dモデル",
    ] {
        let parsed = product_base_parts(title);
        assert_eq!(parsed.name_kind, "fallback", "{title}");
        assert_eq!(parsed.source_latin, None, "{title}");
    }
    for (title, original) in [
        (
            "【3Dモデル】Soraneko -ソラネコ- & Umineko -ウミネコ-【PhysBones対応】",
            "Soraneko -ソラネコ- & Umineko -ウミネコ-",
        ),
        (
            "【 R18 Edition】ソラ(sora) and AAA and ウミ(umi)【オリジナル3Dモデル】",
            "ソラ(sora) and AAA and ウミ(umi)",
        ),
        (
            "オリジナル3Dモデル「ソラネコ、ウミネコ」",
            "ソラネコ、ウミネコ",
        ),
    ] {
        let parsed = product_base_parts(title);
        assert_eq!(parsed.name_kind, "bundle", "{title}");
        assert_eq!(parsed.original, original, "{title}");
    }
}

#[test]
fn base_presentation_keeps_uncertain_labels_and_shared_tag_cleanup_conservative() {
    for source in [
        "星猫 / 海猫",
        "VRChat / 3D Avatar",
        "Original 3D Avatar v1.0",
        "ソラネコ 【別モデル】",
        "ソラネコ (PC Quest)",
    ] {
        let label = structured_base_parts(source);
        assert_eq!(label.name_kind, "fallback", "{source}");
        assert_eq!(label.source_latin, None, "{source}");
        assert!(!label.original.is_empty());
    }
    assert_eq!(
        structured_base_parts("Original 3D Modeler").original,
        "Original 3D Modeler"
    );
    assert_eq!(structured_base_parts("Ren♡Ai").original, "Ren♡Ai");
    for name in [
        "Engel",
        "Freedom",
        "ENG-el",
        "GAMEReady-name",
        "TORIBASE-name",
    ] {
        let parsed = structured_base_parts(name);
        assert_eq!(parsed.original, name);
        assert_eq!(parsed.source_latin.as_deref(), Some(name));
        assert_eq!(parsed.name_kind, "single");
    }
    assert_eq!(
        cleaned_title("アマト【lilToon対応】"),
        "アマト【lilToon対応】"
    );
    assert_eq!(
        base_parts("「真冬」対応コート"),
        ("「真冬」対応コート".into(), None)
    );
    assert_eq!(
        decode_entities("A&amp;B &#32;&#x20; &unknown;"),
        "A&B    &unknown;"
    );
}

#[test]
fn trailing_structural_wrappers_and_plain_bilingual_names_are_consumed() {
    for title in [
        "ソラネコ Soraneko / オリジナル3Dモデル",
        "Original 3D Avatar ソラネコ Soraneko for VRChat",
        "Soraneko ソラネコ / Original 3D Avatar",
        "「ソラネコ」 Soraneko 【オリジナル3Dモデル】",
        "〖VRChat対応〗 ソラネコ (Soraneko) 〔3D Avatar〕",
    ] {
        assert_eq!(
            base_parts(title),
            ("ソラネコ".into(), Some("Soraneko".into())),
            "{title}"
        );
    }
    for title in [
        "ソラネコ 【別モデル】",
        "ソラネコ (対応衣装)",
        "ソラネコ (Soraneko) bonus files",
    ] {
        assert_eq!(base_parts(title), (title.into(), None), "{title}");
    }
}

#[test]
fn slash_wrappers_and_reading_pairs_do_not_invent_multi_avatar_names() {
    for title in [
        "VRChat対応 / オリジナル3Dモデル「星空猫」 / VRChat",
        "オリジナル3Dモデル / 星空猫 / VRChat対応",
        "星空猫 / 星空猫",
        "VRChat対応 星空猫 / VRChat",
    ] {
        assert_eq!(base_parts(title), ("星空猫".into(), None), "{title}");
        assert!(has_name(title));
    }
    for title in [
        "Hoshizoraneko / 星空猫",
        "Original 3D Model / 星空猫 / Hoshizoraneko / VRChat",
    ] {
        assert_eq!(
            base_parts(title),
            ("星空猫".into(), Some("Hoshizoraneko".into()))
        );
        assert!(has_name(title));
    }
    for title in [
        "星猫 / 海猫",
        "オリジナル3Dモデル「星猫」 / オリジナル3Dモデル「海猫」",
        "VRChat / 3D Avatar",
        "星空猫 / 別人",
    ] {
        assert!(!has_name(title), "{title}");
    }
    #[cfg(target_os = "windows")]
    for title in [
        "星空猫 / ホシゾラネコ",
        "ほしぞらねこ / 星空猫 / VRChat対応",
    ] {
        assert_eq!(base_parts(title), ("星空猫".into(), None));
        assert!(has_name(title));
    }
}

#[test]
fn fully_consumed_bilingual_base_titles_have_one_pair() {
    for title in [
        "オリジナル３Ｄモデル「ソラネコ / Soraneko」Ver.1.2",
        "【VRChat対応】オリジナル3Dモデル ソラネコ（Soraneko）",
        "【VRChat対応 / オリジナル3Dモデル】ソラネコ（Soraneko）",
        "ソラネコ -Soraneko- / オリジナル3Dモデル",
        "Original 3D Model “ソラネコ” Soraneko v1.2",
        "ソラネコ — Soraneko",
        "ソラネコ / “Soraneko”",
    ] {
        assert_eq!(
            base_parts(title),
            ("ソラネコ".into(), Some("Soraneko".into())),
            "{title}"
        );
    }
    assert_eq!(
        base_parts("ソラネコ -Sora-Neko-"),
        ("ソラネコ".into(), Some("Sora-Neko".into()))
    );
    assert_eq!(
        base_parts("ソラネコ / Sóraneko"),
        ("ソラネコ".into(), Some("Sóraneko".into()))
    );
}

#[test]
fn ambiguous_products_and_unsupported_letters_are_preserved() {
    for label in [
        "Original 3D Avatar",
        "Original 3D Model",
        "3D Avatar",
        "オリジナル3Dアバター",
    ] {
        assert_eq!(base_parts(label), (label.to_owned(), None));
        assert!(!has_name(label));
    }
    assert_eq!(
        base_parts("【VRChat対応】 Original 3D Avatar"),
        ("Original 3D Avatar".into(), None)
    );
    assert!(!has_name("【VRChat対応】 Original 3D Avatar"));
    for title in [
        "「真冬」対応コート",
        "「真冬」 Jacket",
        "真冬 / Jacket",
        "真冬 / マヌカ",
        "真冬 / Mafuyu / マヌカ",
        "ソラネコ (PC Quest)",
        "ソラネコ / Сора",
        "ソラネコ / Σόρα",
        "ソラネコ / Soraneko))))",
        "そら🧪ねこ",
        "未知漢字",
        "ソラネコ / Soraneko bonus files",
    ] {
        let (original, english) = base_parts(title);
        assert_eq!(original, title, "{title}");
        assert!(english.is_none(), "{title}");
    }
    assert_eq!(
        base_parts("Original 3D Modeler"),
        ("Original 3D Modeler".into(), None)
    );
    assert_eq!(base_parts("Silver2"), ("Silver2".into(), None));
    assert_eq!(base_parts("Version2"), ("Version2".into(), None));
    assert_eq!(base_parts("“V2”"), ("V2".into(), None));
    assert_eq!(base_parts("Original 3D Model V2"), ("V2".into(), None));
    assert_eq!(
        base_parts("【オリジナル3Dモデル】「未知漢字」"),
        ("未知漢字".into(), None)
    );
    assert_eq!(
        base_parts("İİİİソラネコ Ver.1.2"),
        ("İİİİソラネコ".into(), None)
    );
}
