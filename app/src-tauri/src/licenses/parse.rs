//! Reads license facts from public BOOTH item pages and the documents they link. VN3 items
//! count only when a generator answer ends the item exactly; UV terms come from the URL;
//! CC0 needs a declaration about the asset itself. Everything else stays unknown.
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

pub const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWX";

macro_rules! regex {
    ($pattern:expr) => {{
        static CELL: OnceLock<Regex> = OnceLock::new();
        CELL.get_or_init(|| Regex::new($pattern).unwrap())
    }};
}

/// One text block of the item page: the main description or a seller section.
pub struct Block {
    pub heading: String,
    pub text: String,
}

/// A Google document linked from the page, ranked like the research harness.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    DriveFile { id: String, resource_key: Option<String> },
    DriveFolder { id: String, resource_key: Option<String> },
    Document(String),
    Presentation(String),
}

pub struct Candidate {
    pub link: Link,
    pub rank: (u8, u8, u8, u8),
}

/// A 0-yen download control on the page whose variation or file name says it is the license.
pub struct FreeLicense {
    pub route: String,
}

pub struct Page {
    pub blocks: Vec<Block>,
    pub uv: Option<String>,
    pub candidates: Vec<Candidate>,
    pub free_licenses: Vec<FreeLicense>,
}

fn license_word(text: &str) -> bool {
    regex!(r"(?i)利用規約|規約|ライセンス|license|licence|terms|vn3|uvl|使用条款|이용약관|使用許諾|約款|ガイドライン|guideline").is_match(text)
}

/// Text of an element with `<br>` and block ends as line breaks.
fn text_lines(element: ElementRef, skip: Option<ElementRef>) -> String {
    let mut out = String::new();
    for node in element.descendants() {
        if skip.is_some_and(|skip| node.ancestors().any(|a| a.id() == skip.id()) || node.id() == skip.id()) {
            continue;
        }
        match node.value() {
            Node::Text(text) => out.push_str(text),
            Node::Element(e) if e.name() == "br" => out.push('\n'),
            Node::Element(e) if matches!(e.name(), "p" | "div" | "li" | "h2" | "h3" | "h4") => {
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
    out
}

pub fn page(html: &str) -> Page {
    let document = Html::parse_document(html);
    let mut blocks = vec![];
    if let Some(description) = document
        .select(&Selector::parse("div.js-market-item-detail-description").unwrap())
        .next()
    {
        blocks.push(Block { heading: String::new(), text: text_lines(description, None) });
    }
    let heading = Selector::parse("h2,h3,h4").unwrap();
    for section in document.select(&Selector::parse("section.shop__text").unwrap()) {
        let head = section.select(&heading).next();
        blocks.push(Block {
            heading: head.map(|h| h.text().collect::<String>().trim().to_owned()).unwrap_or_default(),
            text: text_lines(section, head),
        });
    }
    let mut uv = None;
    let mut seen = BTreeSet::new();
    let mut candidates = vec![];
    for block in &blocks {
        let mut previous = "";
        for line in block.text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            for found in regex!(r#"https?://[^\s"<>　（）()「」『』【】\[\]、。]+"#).find_iter(line) {
                let raw = found.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
                let Ok(url) = url::Url::parse(&html_unescape(raw)) else { continue };
                if uv.is_none() && uv_url(&url) {
                    uv = Some(url.to_string());
                    continue;
                }
                let Some(link) = google_link(&url) else { continue };
                if !seen.insert(format!("{link:?}")) {
                    continue;
                }
                let label = line.replace(raw, "");
                let label = if label.trim().is_empty() { previous } else { label.trim() };
                let context = license_word(label) || license_word(&block.heading) || license_word(previous);
                let both = format!("{label} {}", block.heading);
                let language = if regex!(r"(?i)(?:^|[^a-z])(?:jp|ja|jpn)(?:$|[^a-z])|日本語|japanese|規約全文|本文|正文").is_match(&both) {
                    0
                } else if regex!(r"(?i)(?:^|[^a-z])(?:en|eng)(?:$|[^a-z])|english|英語").is_match(&both) {
                    2
                } else if other_language(&both) {
                    3
                } else {
                    1
                };
                let skip = !context
                    && regex!(r"(?i)対応アバター|対応モデル|対応素体|使用素材|credits?|クレジット|素材|依存|dependencies|購入者|マニュアル|manual|説明書|導入|手順|how\s*to").is_match(&block.heading);
                let kind = match link {
                    Link::DriveFile { .. } => 0,
                    Link::DriveFolder { .. } => 1,
                    Link::Document(_) => 2,
                    Link::Presentation(_) => 3,
                };
                candidates.push(Candidate { link, rank: (u8::from(!context), u8::from(skip), language, kind) });
            }
            previous = line;
        }
    }
    candidates.sort_by_key(|c| c.rank);
    let mut free_licenses = vec![];
    let name = Selector::parse("div.variation-name").unwrap();
    let control = Selector::parse("a[href^='https://booth.pm/downloadables/']").unwrap();
    for variation in document.select(&Selector::parse("li.variation-item").unwrap()) {
        let variation_name = variation.select(&name).next().map(|n| n.text().collect::<String>()).unwrap_or_default();
        for anchor in variation.select(&control) {
            let href = anchor.value().attr("href").unwrap_or_default();
            let title = anchor.value().attr("title").unwrap_or_default();
            if regex!(r"^https://booth\.pm/downloadables/\d+\?variation_id=\d+$").is_match(href)
                && (license_word(&variation_name) || license_word(title))
            {
                free_licenses.push(FreeLicense { route: href.to_owned() });
            }
        }
    }
    Page { blocks, uv, candidates, free_licenses }
}

pub fn other_language(text: &str) -> bool {
    regex!(r"(?i)(?:^|[^a-z])(?:ko|kr|zh|cn|tw)(?:$|[^a-z])|한국어|이용약관|中文|使用条款|使用條款|繁體|简体|korean|chinese").is_match(text)
}

fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&").replace("\\u0026", "&")
}

fn uv_url(url: &url::Url) -> bool {
    url.host_str().is_some_and(|h| h == "uv-license.com" || h.ends_with(".uv-license.com"))
        && url.path().ends_with("/license")
        && url.query_pairs().any(|(k, _)| k == "utf8" || UV_TERMS.contains(&k.as_ref()))
}

fn google_link(url: &url::Url) -> Option<Link> {
    let host = url.host_str()?;
    let id = |re: &Regex| re.captures(url.path()).map(|c| c[1].to_owned());
    let key = url.query_pairs().find(|(k, _)| k == "resourcekey").map(|(_, v)| v.into_owned());
    match host {
        "drive.google.com" | "drive.usercontent.google.com" => {
            if let Some(id) = id(regex!(r"/file/d/([A-Za-z0-9_-]{10,})")) {
                Some(Link::DriveFile { id, resource_key: key })
            } else if let Some(id) = id(regex!(r"/folders/([A-Za-z0-9_-]{10,})")) {
                Some(Link::DriveFolder { id, resource_key: key })
            } else {
                let id = url.query_pairs().find(|(k, _)| k == "id").map(|(_, v)| v.into_owned())?;
                regex!(r"^[A-Za-z0-9_-]{10,}$").is_match(&id).then_some(Link::DriveFile { id, resource_key: key })
            }
        }
        "docs.google.com" => {
            let c = regex!(r"/(document|presentation)/d/([A-Za-z0-9_-]{10,})").captures(url.path())?;
            Some(if &c[1] == "document" { Link::Document(c[2].to_owned()) } else { Link::Presentation(c[2].to_owned()) })
        }
        _ => None,
    }
}

pub const UV_TERMS: [&str; 8] = ["pcu", "ccu", "ddw", "dmd", "pru", "seu", "veu", "remarks"];

/// A UV term is on only when its last value is `true`, as the UV License site reads it.
pub fn uv_terms(url: &str) -> BTreeMap<String, bool> {
    let pairs: Vec<(String, String)> = url::Url::parse(url)
        .map(|u| u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect())
        .unwrap_or_default();
    UV_TERMS
        .iter()
        .map(|term| {
            let last = pairs.iter().rev().find(|(k, _)| k == term).map(|(_, v)| v.as_str());
            ((*term).to_owned(), last == Some("true"))
        })
        .collect()
}

fn format_char(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
}

/// Whitespace and format characters removed; generator PDFs contain U+200B and some
/// extractors put spaces between characters.
pub fn flat(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !format_char(*c))
        .map(|c| match c {
            '．' => '.',
            '(' => '（',
            ')' => '）',
            c => c,
        })
        .collect()
}

/// Answers of the VN3 generator (v1.11 form, Japanese originals) by stored code.
const ANSWERS: &[(&str, &[&str])] = &[
    ("commercial", &["営利・非営利の目的問わず利用を許可します", "営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します"]),
    ("doujin", &["非営利および非営利有償目的での利用を許可します", "非営利および非営利有償目的での配布等（頒布、送信を含む）を許可します"]),
    ("noncommercial", &["非営利目的に限り許可します", "非営利目的での配布等（頒布、送信を含む）を許可します"]),
    ("private_only", &["私的かつ本人のみによる利用に限り許可します"]),
    ("not_applicable", &["該当するデータではありません"]),
    ("deny_create", &["作成を許可しません"]),
    ("deny_distribute", &["配布等（頒布、送信を含む）を許可しません"]),
    ("ask", &["権利者に個別に問い合わせて下さい", "権利者に個別に問い合わせてください"]),
    ("allow_public", &["許可します（例えばVRChatにてPublicでの公開の許可を含みます）"]),
    ("allow_limited", &["対象を限定しての公開を許可します（例えばVRChatにてPrivateでの公開の許可を含みます）", "対象を限定しての公開を許可します"]),
    ("allow_segregated", &["許可します（ただし棲み分けはおこなうこと）"]),
    ("deny_except_private", &["許可しません（ただし私的使用（プライベートな範囲での利用）については禁止しません）"]),
    ("allow_if_modified", &[
        "オリジナルと異なることが分かる程度に改変した場合は許可します（公式配信と誤解されないため）",
        "オリジナルと異なることが分かる程度に改変した場合は許可します（公式書籍と誤解されないため）",
        "オリジナルと異なることが分かる程度に改変した場合は許可します（公式グッズと誤解されないため）",
        "オリジナルと異なることが分かる程度に改変した場合は許可します（公式ソフトと誤解されないため）",
        "オリジナルと異なることが分かる程度に改変した場合は許可します",
    ]),
    ("free_only_bound", &["無償に限り本利用規約に従わせることを条件に許可します"]),
    ("free_only_users", &["無償に限りユーザー間で行うことを許可します"]),
    ("free_only", &["無償に限り許可します"]),
    ("bound", &["本利用規約に従わせることを条件に許可します"]),
    ("users_only", &["ユーザー間で行うことを許可します"]),
    ("credit_optional_welcome", &["不要ですがあると嬉しいです"]),
    ("credit_not_required", &["不要です"]),
    ("credit_required", &["必要です"]),
    ("allow", &["許可します"]),
    ("deny", &["許可しません"]),
];

/// Item title prefixes that anchor each letter in the Japanese document.
const TITLES: [&str; 24] = [
    "個人による利用", "法人による利用", "自ら利用する目的で、ソーシャル", "自ら利用する目的で、オンラインゲーム",
    "そのプラットフォームにおいて", "性的表現", "暴力的表現", "政治活動", "調整、", "本データまたはパーツを改変",
    "他のデータを改変", "調整または改変を第三者", "未改変状態", "改変した本データ", "映像作品", "出版物",
    "有体物", "製品開発", "本データのメッシュやウェイトをコピー", "本データのメッシュやウェイトを利用せず",
    "本データをそのまま利用しないで", "利用時のクレジット", "権利義務の譲渡", "特記事項",
];

fn answers() -> &'static Vec<(&'static str, String)> {
    static CELL: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    CELL.get_or_init(|| {
        ANSWERS
            .iter()
            .flat_map(|(code, phrases)| phrases.iter().map(move |p| (*code, flat(p).trim_end_matches('。').to_owned())))
            .collect()
    })
}

/// After headings, notes and page numbers are removed, the item must end with a known
/// answer; the longest such answer wins, so text a seller appended leaves it unknown.
fn classify(chunk: &str) -> Option<&'static str> {
    let mut rest = chunk.to_owned();
    for trailer in [
        regex!(r"上記の利用の許可には.*?含まれます。"),
        regex!(r"（[1-8１-８]）(?:利用主体|オンラインサービスへのアップロード|センシティブな表現|加工|再配布・配布|メディア・プロダクトへの利用|二次創作|その他)"),
        regex!(r"[3３]\.特記事項"),
        regex!(r"[0-9０-９]+/[0-9０-９]+"),
    ] {
        rest = trailer.replace_all(&rest, "").into_owned();
    }
    let rest = rest.trim_end_matches(['。', '.']);
    answers()
        .iter()
        .filter(|(_, phrase)| rest.ends_with(phrase.as_str()))
        .max_by_key(|(_, phrase)| phrase.len())
        .map(|(code, _)| *code)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vn3 {
    pub version: Option<String>,
    pub answers: BTreeMap<String, String>,
}
impl Vn3 {
    pub fn complete(&self) -> bool {
        self.answers.len() == LETTERS.len()
    }
}

/// A Japanese VN3 generator document, or `None` for anything else.
pub fn vn3(text: &str) -> Option<Vn3> {
    let flat = flat(text);
    if !flat.contains("VN3") && !flat.contains("ＶＮ３") {
        return None;
    }
    if !flat.contains("個別条件") && !flat.contains("個人による利用") {
        return None;
    }
    let version = regex!(r"VN3(?:ライセンス|License)Ver\.?([0-9.]*[0-9])")
        .captures(&flat)
        .or_else(|| regex!(r"(?:ジェネレータ|Generator)Ver\.?([0-9.]*[0-9])").captures(&flat))
        .map(|c| c[1].to_owned());
    let letters: Vec<char> = LETTERS.chars().collect();
    let mut positions: Vec<(usize, usize)> = vec![];
    let mut from = 0;
    for (index, letter) in letters.iter().enumerate() {
        let needle = format!("{letter}.{}", flat_title(index));
        if let Some(found) = flat[from..].find(&needle) {
            positions.push((index, from + found));
            from += found + 1;
        }
    }
    let mut result = BTreeMap::new();
    for (i, (index, at)) in positions.iter().enumerate() {
        let start = at + 2 + flat_title(*index).len();
        let next = positions.get(i + 1);
        // A missing next item means this chunk may run into its text and end with
        // its answer, so an item followed by a gap stays unknown.
        if next.is_some_and(|(n, _)| *n != index + 1) {
            continue;
        }
        let end = next.map(|(_, at)| *at).unwrap_or_else(|| {
            flat[start..].char_indices().nth(600).map(|(offset, _)| start + offset).unwrap_or(flat.len())
        });
        if end < start {
            continue;
        }
        let chunk = &flat[start..end];
        let code = if letters[*index] == 'X' {
            let body = regex!(r"[4４]\.権利者").split(chunk).next().unwrap_or_default();
            // Only an explicit "なし" says there are none; an empty body means the
            // extractor put the notes elsewhere.
            if regex!(r"^(?:特に)?(?:なし|無し|ありません)").is_match(body) {
                Some("none")
            } else if !body.is_empty() {
                Some("present")
            } else {
                None
            }
        } else {
            classify(chunk)
        };
        if let Some(code) = code {
            result.insert(letters[*index].to_string(), code.to_owned());
        }
    }
    (!result.is_empty()).then_some(Vn3 { version, answers: result })
}

fn flat_title(index: usize) -> &'static str {
    static CELL: OnceLock<Vec<String>> = OnceLock::new();
    &CELL.get_or_init(|| TITLES.iter().map(|t| flat(t)).collect())[index]
}

/// NFKC with runs of spaces collapsed, as the CC0 rule reads each line.
fn nf(text: &str) -> String {
    let text: String = text.nfkc().collect();
    regex!(r"[ \t\u{3000}]+").replace_all(&text, " ").trim().to_owned()
}

fn cc0_token(text: &str) -> bool {
    regex!(r"(?i)(?:^|[^a-z0-9])cc\s*-?\s*0(?:$|[^0-9])|creative\s*commons\s*zero|クリエイティブ・?コモンズ・?ゼロ").is_match(text)
}

fn cc0_declaration(line: &str) -> bool {
    [
        regex!(r#"(?i)(?:ライセンス|license|licence|利用規約|規約|著作権|copyright)\s*(?:は|[:：]|は[:：]|=|-|・)?\s*[「『【\[(（"]?\s*(?:cc\s*-?\s*0|creative\s*commons\s*zero|クリエイティブ・?コモンズ・?ゼロ)"#),
        regex!(r"(?i)cc\s*-?\s*0(?:\s*1\.0)?\s*(?:ライセンス|license|licence)?\s*(?:で|にて|として|の(?:下|もと)で)\s*(?:無料)?(?:配布|公開|提供|頒布|販売|リリース)"),
        regex!(r"(?i)(?:本|この|当|こちらの)(?:商品|作品|データ|モデル|アセット|素材|コンテンツ|ツール|パッケージ|アイテム|ワールド|シェーダー?|ギミック|テクスチャ|アバター|衣装|小物|プレハブ|モーション|アニメーション|フォント|音源)(?:\s*(?:は|を|も|につきましては|については))?[^。!！]{0,14}?cc\s*-?\s*0"),
        regex!(r"(?i)cc\s*-?\s*0(?:\s*1\.0)?\s*(?:ライセンス)?\s*(?:です|とします|とし[、,。]|とする|にします|に変更|に設定|を適用|が適用|で(?:す|ございます))"),
        regex!(r"(?i)(?:released|licensed|distributed|published|provided|available|shared)\s+(?:under|as|with)\s+(?:the\s+)?(?:cc\s*-?\s*0|creative\s*commons\s*zero)"),
        regex!(r"(?i)(?:is|are)\s+(?:licensed\s+)?(?:under\s+)?cc\s*-?\s*0\b"),
        regex!(r"(?i)cc\s*-?\s*0(?:\s*1\.0)?\s*[(（][^)）]{0,30}[)）]\s*(?:です|とします|で(?:配布|公開|提供))"),
        regex!(r"(?i)cc\s*-?\s*0(?:ライセンス)?\s*(?:なので|の為|のため|につき)[^。]{0,24}(?:自由|好きに|商用|再配布|改変|ご利用|使って|使用でき|ok|可能)"),
        regex!(r"(?i)(?:データ|ファイル|モデル|コンテンツ|パッケージ)(?:に関しては|については|は)\s*cc\s*-?\s*0"),
    ]
    .iter()
    .any(|re| re.is_match(line))
}

/// What a sentence calls CC0 is the asset itself, its download data or its license, or nothing
/// is named ("CC0です"). Any other subject, such as bundled textures, is about a part.
fn about_the_asset(sentence: &str) -> bool {
    let Some(token) = regex!(r"(?i)(?:^|[^a-z0-9])(cc\s*-?\s*0)(?:$|[^0-9])|(creative\s*commons\s*zero)|(クリエイティブ・?コモンズ・?ゼロ)").captures(sentence) else {
        return false;
    };
    let start = (1..=3).find_map(|group| token.get(group)).map_or(0, |found| found.start());
    let subject = sentence[..start].to_lowercase();
    regex!(
        r"(?x)^[\W_]*
        (?:
            (?:(?:本|この|当|こちらの)\s*(?:商品|作品|データ|モデル|アセット|素材|コンテンツ|ツール|パッケージ|アイテム|ワールド|シェーダー?|ギミック|テクスチャ|アバター|衣装|小物|プレハブ|モーション|アニメーション|フォント|音源|vrm)
              |本作|こちら|this\s+(?:asset|product|item|model|package|work|content|texture|shader|font|tool|world|avatar)|it|everything)
            (?:\s*の\s*(?:ダウンロード\s*)?(?:データ|ファイル))?
            (?:\s*の\s*(?:ライセンス(?:形態)?|利用規約|規約|著作権))?
          |(?:ダウンロード\s*)?データ
          |ライセンス(?:形態)?|license|licence|利用規約|規約|著作権|copyright
        )?
        [\W_]*(?:は|も|が|を|に関しては|については|に関して|について|として|is|are)?
        [\W_]*(?:全て|すべて|全部|完全に|もちろん)?
        \s*(?:(?:is|are)\s+)?(?:(?:released|licensed|distributed|published|provided|available|shared)\s+)?(?:(?:under|as|with)\s+)?(?:the\s+)?
        [\W_]*$"
    )
    .is_match(&subject)
}

/// Only an explicit declaration that this asset is CC0 counts. Titles and tags alone, CC0
/// materials, parts of the asset, products made for a CC0 base, and any carve-out line on
/// the page do not.
pub fn cc0(blocks: &[Block]) -> bool {
    let mut declared = false;
    for block in blocks {
        let heading = nf(&block.heading);
        for raw in std::iter::once(block.heading.as_str()).chain(block.text.lines()) {
            let line = nf(raw);
            if line.is_empty() || !cc0_token(&line) {
                continue;
            }
            if regex!(r"(?i)ではありません|ではない|ではなく|では無い|じゃない|適用されません|適用外|対象外|対象には含まれ|含まれません|以外|除く|除き|not\s+(?:a\s+)?cc\s*-?\s*0|non-?cc0|except|excluding|does\s+not\s+apply").is_match(&line) {
                return false;
            }
            let about_other = regex!(r"(?i)(?:を|も|が)(?:使用|利用)(?:して|させて|した|しています|されて)|使用して(?:い|お)|使用素材|使用している|お借り|引用|から(?:の|お借り)|配布元|(?:素体|アバター|モデル|avatar|model|base)[^。]{0,8}(?:対応|向け|用|専用|for)|(?:対応|向け|専用)(?:素体|アバター|モデル)|さん|様|氏の|作者|by\s").is_match(&line);
            let deed = regex!(r"(?i)^[\W_]*(?:cc\s*-?\s*0|creative\s*commons\s*zero)\s*(?:1\.0)?\s*(?:universal|全世界|public\s*domain|パブリック\s*ドメイン)?[\W_]*(?:\(?cc\s*0\s*1\.0\)?)?[\W_]*$").is_match(&line)
                && regex!(r"(?i)ライセンス|license|licence|利用規約|規約|terms|著作権|copyright").is_match(&heading)
                && line != heading;
            // Each sentence must both declare and be about the asset itself.
            let stated = line.split(['。', '！', '!', '？', '?']).any(|s| cc0_token(s) && cc0_declaration(s) && about_the_asset(s));
            if !about_other && (stated || deed) {
                declared = true;
            }
        }
    }
    declared
}
