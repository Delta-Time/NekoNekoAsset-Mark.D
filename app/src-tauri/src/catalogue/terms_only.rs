//! A free download that holds only a listing's terms of use. Sellers of paid assets often
//! add a 0-yen variation with just the terms, so buyers can read them first; BOOTH records
//! downloading it as a free download of the whole listing, which then sits in the library
//! as an asset that is nothing but the terms.
//!
//! Hiding a real free asset would be worse than showing one of these, so every condition
//! must hold: the listing also sells a paid variation, and each file is the terms. A file
//! is the terms when its own name says so and it is a document, an archive or an image,
//! or when it is a document from a 0-yen variation whose name says so; a name that also
//! names something else (a trial, a manual, a patch, photos, royalty-free material, a
//! template, an asset that comes with its terms) is not. A free listing whose only file is
//! a document is a free document, and stays.
//!
//! Public listings sampled on 2026-11-23: of 25 paid listings with a 0-yen variation,
//! three offered only their terms, each as a ZIP whose own name said so.
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

/// One listed file: its name and the variation it comes from, when known.
pub(crate) struct TermsFile<'a> {
    pub name: &'a str,
    pub variant_id: Option<&'a str>,
}

fn normalized(text: &str) -> String {
    text.nfkc().collect::<String>().to_lowercase()
}

/// Words that name terms of use. A guideline is left out: guides are sold as assets.
fn terms_word(text: &str) -> bool {
    let text = normalized(text);
    [
        "利用規約",
        "規約",
        "ライセンス",
        "license",
        "licence",
        "terms",
        "vn3",
        "使用許諾",
        "約款",
        "条款",
        "條款",
        "이용약관",
        "kiyaku",
    ]
    .iter()
    .any(|word| text.contains(word))
}

/// Words that name something besides the terms in the same file or variation, matched
/// across the spaces, `_`, `-`, `.` and `・` that names put between words.
fn content_word(text: &str) -> bool {
    let text: String = normalized(text)
        .chars()
        .filter(|c| !matches!(c, ' ' | '_' | '-' | '.' | '・'))
        .collect();
    [
        "体験",
        "お試し",
        "試用",
        "無料版",
        "本体",
        "サンプル",
        "sample",
        "demo",
        "trial",
        "unitypackage",
        "説明書",
        "マニュアル",
        "manual",
        "パッチ",
        "patch",
        "写真",
        "photo",
        "テクスチャ",
        "texture",
        // Royalty-free material, a terms template, or an asset that comes with its terms.
        "ライセンスフリー",
        "licensefree",
        "royalty",
        "ロイヤリティ",
        "ロイヤルティ",
        "テンプレ",
        "template",
        "同梱",
        "付き",
    ]
    .iter()
    .any(|word| text.contains(word))
}

enum Format {
    Document,
    /// An archive or an image, which only its own name can say is the terms.
    Named,
    Other,
}

fn format(name: &str) -> Format {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" | "txt" | "md" | "html" | "htm" | "rtf" | "doc" | "docx" | "odt" => Format::Document,
        "zip" | "7z" | "rar" | "png" | "jpg" | "jpeg" | "webp" => Format::Named,
        _ => Format::Other,
    }
}

fn price(variant: &Value) -> Option<f64> {
    variant["price"].as_f64()
}

/// Whether a listing whose public variations are `variants` sells one for a price: only then
/// can its free files be only its terms. The projection keeps it as `listingSellsPaid`, so
/// a listing that starts or stops selling one refreshes its list row.
pub(crate) fn sells_paid(variants: &Value) -> bool {
    variants
        .as_array()
        .is_some_and(|variants| variants.iter().any(|v| price(v).is_some_and(|p| p > 0.0)))
}

/// Whether `files` hold only the terms of a listing whose public variations are `variants`
/// (BOOTH's `publicVariants`).
pub(crate) fn terms_only(files: &[TermsFile<'_>], variants: &Value) -> bool {
    let sells_paid = sells_paid(variants);
    let variants = variants.as_array().map(Vec::as_slice).unwrap_or_default();
    let variation =
        |id: Option<&str>| id.and_then(|id| variants.iter().find(|v| v["id"].as_str() == Some(id)));
    let terms = |file: &TermsFile<'_>| {
        let variation = variation(file.variant_id);
        let variation_name = variation.and_then(|v| v["name"].as_str()).unwrap_or("");
        if content_word(file.name) || content_word(variation_name) {
            return false;
        }
        match format(file.name) {
            Format::Document => {
                terms_word(file.name)
                    || variation.is_some_and(|v| price(v) == Some(0.0))
                        && terms_word(variation_name)
            }
            Format::Named => terms_word(file.name),
            Format::Other => false,
        }
    };
    sells_paid && !files.is_empty() && files.iter().all(terms)
}

#[cfg(test)]
#[path = "terms_only_tests.rs"]
mod tests;
