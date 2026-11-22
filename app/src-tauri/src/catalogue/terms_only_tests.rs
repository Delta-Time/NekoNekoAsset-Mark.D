use super::*;
use serde_json::json;

fn file(name: &str) -> TermsFile<'_> {
    TermsFile {
        name,
        variant_id: None,
    }
}

fn from<'a>(name: &'a str, variant: &'a str) -> TermsFile<'a> {
    TermsFile {
        name,
        variant_id: Some(variant),
    }
}

/// A paid listing with a 0-yen terms variation, as sellers set them up.
fn listing() -> Value {
    json!([
        {"id":"800001","name":"利用規約","type":"digital","price":0,"currency":"JPY"},
        {"id":"800002","name":"本体","type":"digital","price":1500,"currency":"JPY"},
        {"id":"800003","name":"利用規約付き本体","type":"digital","price":2000,"currency":"JPY"}
    ])
}

#[test]
fn a_paid_listings_free_terms_are_terms_only() {
    for name in [
        "利用規約.pdf",
        "VN3ライセンス_ja.pdf",
        "Terms_of_Use.txt",
        "LICENSE.md",
        "kiyaku.PDF",
        "ＬＩＣＥＮＳＥ.pdf",
        "이용약관.pdf",
        "使用条款.txt",
    ] {
        assert!(terms_only(&[file(name)], &listing()), "{name}");
    }
    // Terms often come zipped, or as a picture, named for what they are.
    for name in [
        "利用規約.zip",
        "20250212vn3license_zh.zip",
        "利用規約_Authored.7z",
        "利用規約.png",
    ] {
        assert!(terms_only(&[file(name)], &listing()), "{name}");
    }
    // A document named for nothing still counts when its 0-yen variation names the terms.
    assert!(terms_only(&[from("readme.txt", "800001")], &listing()));
    // Terms in two languages are still only terms.
    assert!(terms_only(
        &[file("利用規約_ja.pdf"), file("terms_en.pdf")],
        &listing()
    ));
}

#[test]
fn anything_besides_the_terms_keeps_the_asset() {
    for files in [
        // An archive or an image named for something else may hold the asset itself,
        // even from a 0-yen terms variation.
        vec![from("Authored.zip", "800001")],
        vec![from("preview.png", "800001")],
        vec![file("利用規約.pdf"), file("sample.zip")],
        vec![file("Authored.unitypackage")],
        // A name that also names a trial, a manual, a patch or photos is more than terms.
        vec![file("体験版_利用規約同梱.zip")],
        vec![file("利用規約・説明書.pdf")],
        vec![file("license_patch.zip")],
        vec![file("利用規約_photo.zip")],
        // A document that names no terms is a manual or a guide.
        vec![file("manual.pdf")],
        vec![file("撮影ガイドライン.pdf")],
        vec![file("利用規約.pdf"), file("説明書.pdf")],
        // Only a 0-yen variation speaks for an unnamed file.
        vec![from("readme.txt", "800003")],
        vec![from("readme.txt", "800009")],
    ] {
        let names: Vec<_> = files.iter().map(|file| file.name).collect();
        assert!(!terms_only(&files, &listing()), "{names:?}");
    }
    assert!(!terms_only(&[], &listing()));
}

#[test]
fn a_listing_that_sells_nothing_is_never_terms_only() {
    // A free document is the asset itself.
    let free = json!([{"id":"800001","name":"利用規約","type":"digital","price":0}]);
    assert!(!terms_only(
        &[from("利用規約テンプレート.docx", "800001")],
        &free
    ));
    // A price nobody read proves no paid variation, nor do missing variations.
    let unread = json!([
        {"id":"800001","name":"利用規約","type":"digital","price":0},
        {"id":"800002","name":"本体","type":"digital"}
    ]);
    assert!(!terms_only(&[file("利用規約.pdf")], &unread));
    assert!(!terms_only(&[file("利用規約.pdf")], &Value::Null));
}
