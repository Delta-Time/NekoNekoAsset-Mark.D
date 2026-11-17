use super::*;

/// An authored document in the generator's layout: item titles, then the chosen answers.
fn document(answers: &[(&str, &str, &str)]) -> String {
    let mut text = String::from("VN3ライセンス Ver.1.10\n利用規約\n2. 個別条件\n");
    for (letter, title, answer) in answers {
        text.push_str(&format!("{letter}. {title}\n{answer}\n"));
    }
    text.push_str("4. 権利者\n");
    text
}

fn full() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("A", "個人による利用", "非営利目的に限り許可します"),
        ("B", "法人による利用", "権利者に個別に問い合わせて下さい"),
        ("C", "自ら利用する目的で、ソーシャルVRへアップロード", "許可します"),
        ("D", "自ら利用する目的で、オンラインゲームへアップロード", "許可しません"),
        ("E", "そのプラットフォームにおいて第三者に利用させる", "対象を限定しての公開を許可します（例えばVRChatにてPrivateでの公開の許可を含みます）"),
        ("F", "性的表現への利用", "許可します（ただし棲み分けはおこなうこと）"),
        ("G", "暴力的表現への利用", "許可しません（ただし私的使用（プライベートな範囲での利用）については禁止しません）"),
        ("H", "政治活動・宗教活動への利用", "許可しません"),
        ("I", "調整、ポリゴン削減", "許可します"),
        ("J", "本データまたはパーツを改変", "許可します"),
        ("K", "他のデータを改変する目的での利用", "許可します"),
        ("L", "調整または改変を第三者に依頼", "ユーザー間で行うことを許可します"),
        ("M", "未改変状態での再配布", "許可しません"),
        ("N", "改変した本データの配布", "無償に限り本利用規約に従わせることを条件に許可します"),
        ("O", "映像作品への利用", "許可します"),
        ("P", "出版物への利用", "オリジナルと異なることが分かる程度に改変した場合は許可します（公式書籍と誤解されないため）"),
        ("Q", "有体物への利用", "権利者に個別に問い合わせてください"),
        ("R", "製品開発への利用", "許可しません"),
        ("S", "本データのメッシュやウェイトをコピーして衣装を作成", "私的かつ本人のみによる利用に限り許可します"),
        ("T", "本データのメッシュやウェイトを利用せず衣装を作成", "該当するデータではありません"),
        ("U", "本データをそのまま利用しないで二次創作", "営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します"),
        ("V", "利用時のクレジット表記", "不要ですがあると嬉しいです"),
        ("W", "権利義務の譲渡", "許可しません"),
        ("X", "特記事項", "なし"),
    ]
}

#[test]
fn vn3_items_read_only_exact_generator_answers() {
    let parsed = parse::vn3(&document(&full())).unwrap();
    assert!(parsed.complete());
    assert_eq!(parsed.version.as_deref(), Some("1.10"));
    for (letter, code) in [("A", "noncommercial"), ("B", "ask"), ("E", "allow_limited"), ("F", "allow_segregated"), ("G", "deny_except_private"), ("L", "users_only"), ("N", "free_only_bound"), ("P", "allow_if_modified"), ("Q", "ask"), ("S", "private_only"), ("T", "not_applicable"), ("U", "commercial"), ("V", "credit_optional_welcome"), ("X", "none")] {
        assert_eq!(parsed.answers[letter], code, "{letter}");
    }
    // Zero-width characters and spaced-out letters from PDF extraction read the same.
    let spaced: String = document(&full()).chars().flat_map(|c| [c, '\u{200b}', ' ']).collect();
    assert_eq!(parse::vn3(&spaced).unwrap().answers, parsed.answers);

    // Seller text after an answer, a missing item and an empty note leave those unknown.
    let mut items = full();
    items[2].2 = "許可します。ただしR-18ワールドは除きます";
    items.remove(9);
    items[22].2 = "";
    let parsed = parse::vn3(&document(&items)).unwrap();
    assert!(!parsed.answers.contains_key("C"));
    assert!(!parsed.answers.contains_key("I"), "the item before a gap could hold the next answer");
    assert!(!parsed.answers.contains_key("J"));
    assert_eq!(parsed.answers["K"], "allow");
    assert!(!parsed.answers.contains_key("X"));
    let mut noted = full();
    noted[23].2 = "商用利用時はご連絡ください";
    assert_eq!(parse::vn3(&document(&noted)).unwrap().answers["X"], "present");

    // Not VN3, or not the Japanese document: nothing.
    assert!(parse::vn3("利用規約\nA. 個人による利用\n許可します").is_none());
    assert!(parse::vn3("VN3 License Ver.1.10\nA. Personal use\nAllowed").is_none());
}

#[test]
fn cells_follow_the_agreed_reading() {
    let parsed = parse::vn3(&document(&full())).unwrap();
    let vn3 = cells("vn3", &json!(parsed.answers));
    for (letter, value) in [("A", "allow"), ("B", "deny"), ("C", "allow"), ("D", "deny"), ("E", "allow"), ("F", "allow"), ("G", "deny"), ("L", "allow"), ("N", "allow"), ("P", "allow"), ("Q", "deny"), ("S", "deny"), ("U", "allow"), ("V", "notRequired"), ("X", "no")] {
        assert_eq!(vn3[letter], value, "{letter}");
    }
    assert!(!vn3.contains_key("T"), "not applicable stays unknown");

    let uv = cells("uv", &json!(parse::uv_terms("https://uv-license.com/ja/license?utf8=%E2%9C%93&pcu=true&ccu=false&ccu=true&ddw=true&seu=1&remarks=true")));
    for (letter, value) in [("A", "allow"), ("B", "allow"), ("F", "deny"), ("G", "deny"), ("M", "deny"), ("N", "allow"), ("S", "deny"), ("U", "allow"), ("V", "required"), ("X", "yes")] {
        assert_eq!(uv[letter], value, "{letter}");
    }
    for letter in ["D", "E", "O", "P", "Q", "R", "T", "W"] {
        assert!(!uv.contains_key(letter), "{letter}");
    }

    let cc0 = cells("cc0", &json!({}));
    assert_eq!(cc0.len(), 24);
    assert_eq!((cc0["A"].as_str(), cc0["M"].as_str(), cc0["V"].as_str(), cc0["X"].as_str()), ("allow", "allow", "notRequired", "no"));

    assert_eq!(valid_cells(&json!({"A":"allow","V":"allow","X":"yes","Y":"allow","B":"maybe","C":1})), BTreeMap::from([("A".into(), "allow".into()), ("X".into(), "yes".into())]));
}

fn page_with(description: &str, sections: &[(&str, &str)], variations: &str) -> String {
    let sections: String = sections.iter().map(|(h, p)| format!("<section class='shop__text'><h2>{h}</h2><p class='js-autolink'>{p}</p></section>")).collect();
    format!("<html><body><div class='js-market-item-detail-description description'><p>{description}</p></div>{sections}<ul>{variations}</ul></body></html>")
}

#[test]
fn page_links_rank_license_documents_first() {
    let html = page_with(
        "説明\nマニュアル https://drive.google.com/file/d/manualmanual01/view",
        &[
            ("利用規約", "English https://drive.google.com/file/d/englishterms01/view\n日本語 https://drive.google.com/file/d/japaneseterm1/view?usp=sharing\nhttps://docs.google.com/document/d/termsdocument1/edit"),
            ("配布", "https://drive.google.com/drive/folders/sharedfolder01?resourcekey=0-abc"),
        ],
        "<li class='variation-item'><div class='variation-name'>利用規約</div><a title='terms.pdf' href='https://booth.pm/downloadables/700001?variation_id=800001'>無料ダウンロード</a></li><li class='variation-item'><div class='variation-name'>本体</div><a title='model.zip' href='https://booth.pm/downloadables/700002?variation_id=800002'>無料ダウンロード</a></li>",
    );
    let page = parse::page(&html);
    let links: Vec<_> = page.candidates.iter().map(|c| c.link.clone()).collect();
    assert_eq!(links[0], parse::Link::DriveFile { id: "japaneseterm1".into(), resource_key: None });
    assert_eq!(links[1], parse::Link::Document("termsdocument1".into()));
    assert_eq!(links[2], parse::Link::DriveFile { id: "englishterms01".into(), resource_key: None });
    // Links outside license wording come after every license-labelled one.
    assert_eq!(links[3..].len(), 2);
    assert!(links[3..].contains(&parse::Link::DriveFolder { id: "sharedfolder01".into(), resource_key: Some("0-abc".into()) }));
    assert!(links[3..].contains(&parse::Link::DriveFile { id: "manualmanual01".into(), resource_key: None }));
    assert!(page.uv.is_none());
    assert_eq!(page.free_licenses.iter().map(|f| f.route.as_str()).collect::<Vec<_>>(), ["https://booth.pm/downloadables/700001?variation_id=800001"]);

    let uv = parse::page(&page_with("ライセンス\nhttps://uv-license.com/ja/license?utf8=✓&pcu=true&remarks= も見てください\nhttps://uv-license.com/ja/", &[], ""));
    assert!(uv.uv.as_deref().is_some_and(|u| u.contains("pcu=true")));
    assert!(parse::page(&page_with("UVライセンス https://uv-license.com/ja/license", &[], "")).uv.is_none(), "a bare link names no terms");
}

#[test]
fn cc0_needs_a_declaration_about_the_asset() {
    let judge = |description: &str, sections: &[(&str, &str)]| parse::cc0(&parse::page(&page_with(description, sections, "")).blocks);
    for declared in [
        "本商品はCC0で配布します。",
        "ライセンス：CC0",
        "This asset is released under CC0.",
        "このシェーダーのライセンスはCC0です",
        "CC0なので自由に改変・再配布できます",
        "本商品のダウンロードデータはCC0です。",
        "ご自由にお使いください。CC0ライセンスです。",
        "・本パッケージもCC0です",
    ] {
        assert!(judge(declared, &[]), "{declared}");
    }
    assert!(judge("説明", &[("ライセンス", "CC0 1.0 Universal")]));
    for not in [
        "【CC0】テクスチャ素材",
        "CC0の素材を使用しています",
        "CC0素体対応の衣装です",
        "〇〇さんのCC0モデルを改変しました",
        "CC0",
        // A part of the asset is not the asset.
        "付属のテクスチャはCC0です",
        "同梱HDRIはCC0です",
        "この商品のテクスチャはCC0です",
        "本商品に付属のフォントはCC0で配布します",
        "The included textures are CC0.",
        "各種テクスチャがCC0で配布されているサイトです。",
    ] {
        assert!(!judge(not, &[]), "{not}");
    }
    // One carve-out anywhere on the page outweighs every declaration.
    assert!(!judge("本商品はCC0で配布します。\n同梱のフォントはCC0ではありません", &[]));
    assert!(!judge("ライセンス：CC0", &[("注意", "サンプル画像はCC0の対象外です")]));
}

#[test]
fn zip_and_text_documents_are_read_and_others_ignored() {
    let text = document(&full());
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("license/vn3license_ja.txt", zip::write::SimpleFileOptions::default()).unwrap();
    std::io::Write::write_all(&mut zip, format!("\u{feff}{text}").as_bytes()).unwrap();
    zip.start_file("model.fbx", zip::write::SimpleFileOptions::default()).unwrap();
    std::io::Write::write_all(&mut zip, b"binary").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let texts = fetch::file_texts(&bytes);
    assert_eq!(texts.len(), 1);
    assert!(parse::vn3(&texts[0]).unwrap().complete());
    assert_eq!(fetch::file_texts(text.as_bytes()).len(), 1);
    assert!(fetch::file_texts(b"<!DOCTYPE html><html><body>Only the owner can download</body></html>").is_empty());
    assert!(fetch::file_texts(b"%PDF-1.4 broken").is_empty());
}

#[test]
fn booth_links_name_one_item() {
    assert_eq!(booth_item("https://booth.pm/ja/items/1234567").as_deref(), Some("1234567"));
    assert_eq!(booth_item("https://shop.booth.pm/items/1234567").as_deref(), Some("1234567"));
    for other in ["http://booth.pm/ja/items/1", "https://booth.pm/ja/items/1/reviews", "https://example.com/items/1", "https://booth.pm/ja/items/abc"] {
        assert!(booth_item(other).is_none(), "{other}");
    }
}

#[test]
fn a_failed_read_never_replaces_a_result_and_edits_win() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("licenses-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&dir).unwrap();
    let service = LicenseService::new(catalogue.clone(), BoothService::new(catalogue).unwrap());
    assert_eq!(service.matching(&json!({"A":"allow"})).unwrap(), Some(vec![]));
    assert!(!service.inner.path.exists(), "a filter alone creates nothing");
    let found = |kind: &'static str, answers: Value| fetch::Outcome::Found(fetch::Found { kind, version: None, source: String::new(), answers });
    service.record("vn3", found("vn3", json!({"A":"commercial","B":"ask"}))).unwrap();
    service.record("cc0", found("cc0", json!({}))).unwrap();
    service.record("none", fetch::Outcome::Missing).unwrap();
    service.record("vn3", fetch::Outcome::Failed("NETWORK_UNAVAILABLE".into())).unwrap();
    service.record("cc0", fetch::Outcome::Missing).unwrap();
    assert_eq!(service.matching(&json!({"A":"allow"})).unwrap(), Some(vec!["cc0".to_owned(), "vn3".to_owned()]));
    assert_eq!(service.matching(&json!({"A":"allow","B":"deny"})).unwrap(), Some(vec!["vn3".to_owned()]));
    assert_eq!(service.matching(&json!({"Z":"allow"})).unwrap(), None);
    assert_eq!(service.settled("state IN('found','none')").unwrap(), BTreeSet::from(["cc0".into(), "none".into(), "vn3".into()]));

    // A user's table replaces the synced one until it is cleared.
    service.with_db(true, |conn| Ok(conn.execute("INSERT INTO edits(product_id,cells,edited_at) VALUES('cc0','{\"A\":\"deny\"}','now')", [])?)).unwrap();
    assert_eq!(service.matching(&json!({"A":"allow"})).unwrap(), Some(vec!["vn3".to_owned()]));
    assert_eq!(service.matching(&json!({"M":"allow"})).unwrap(), Some(vec![]));
    drop(service);
    std::fs::remove_dir_all(dir).unwrap();
}
