//! Reads one asset's license: its public BOOTH page, then the Google documents it links and
//! its 0-yen license download, then a CC0 declaration. A failed request is never a result.
use super::parse::{self, Link, Vn3};
use crate::booth::BoothService;
use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub const DOCUMENT_LIMIT: u64 = 25 * 1024 * 1024;
/// Downloads and listings per asset, as in the research run that found 96% of VN3 documents.
const TRIES: usize = 4;

pub struct Found {
    pub kind: &'static str,
    pub version: Option<String>,
    pub source: String,
    pub answers: Value,
}
pub enum Outcome {
    Found(Found),
    /// The page was read and holds no license this App reads.
    Missing,
    /// Something could not be read this time; earlier results stay.
    Failed(String),
}

/// Downloads a 0-yen license file with an account that holds the product.
pub trait FreeDownload: Sync {
    fn download<'a>(
        &'a self,
        route: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send + 'a>>;
}

/// A document's texts, or why there are none. Only `Retry` is worth another attempt.
#[derive(Clone)]
enum Texts {
    Read(Vec<String>),
    Unreadable,
    Retry,
}

pub struct Reader {
    booth: BoothService,
    public: Client,
    google: Client,
    pace: Mutex<Instant>,
    documents: Mutex<HashMap<String, Texts>>,
}

fn google_host(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|h| {
            matches!(
                h,
                "drive.google.com" | "drive.usercontent.google.com" | "docs.google.com"
            ) || h.ends_with(".googleusercontent.com")
        })
}

impl Reader {
    pub fn new(booth: BoothService) -> Result<Self> {
        let policy = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 {
                attempt.error("REDIRECT_LIMIT")
            } else if google_host(attempt.url()) {
                attempt.follow()
            } else {
                // A sign-in page or another host: the document is not public.
                attempt.stop()
            }
        });
        let google = Client::builder()
            .redirect(policy)
            .referer(false)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!(
                "NekoNekoAsset/",
                env!("CARGO_PKG_VERSION"),
                " (Windows; private asset manager)"
            ))
            .build()
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        Ok(Self {
            public: booth.public_client()?,
            booth,
            google,
            pace: Mutex::new(Instant::now()),
            documents: Mutex::new(HashMap::new()),
        })
    }

    /// `holder` downloads the product's 0-yen license file when the documents fall short.
    pub async fn read(&self, item: &str, holder: Option<&dyn FreeDownload>) -> Outcome {
        let (status, html) = match self.booth.public_item_page(&self.public, item).await {
            Ok(page) => page,
            Err(error) => return Outcome::Failed(crate::booth::error_code(&error)),
        };
        if matches!(status, 404 | 410) {
            return Outcome::Missing;
        }
        if !(200..300).contains(&status) {
            return Outcome::Failed(format!("HTTP_{status}"));
        }
        let page = parse::page(&html);
        let item_url = format!("https://booth.pm/ja/items/{item}");
        if let Some(uv) = page.uv {
            let answers = json!(parse::uv_terms(&uv));
            return Outcome::Found(Found {
                kind: "uv",
                version: None,
                source: uv,
                answers,
            });
        }
        let mut best: Option<(Vn3, String)> = None;
        let mut retry = false;
        // Why the 0-yen download failed, when that failure is worth another sync.
        let mut refused: Option<String> = None;
        let mut tries = 0;
        let better = |v: &Vn3, best: &Option<(Vn3, String)>| {
            best.as_ref().is_none_or(|(b, _)| {
                (v.complete(), v.answers.len()) > (b.complete(), b.answers.len())
            })
        };
        for candidate in page.candidates.iter().take(6) {
            if tries >= TRIES || best.as_ref().is_some_and(|(v, _)| v.complete()) {
                break;
            }
            // Another language's copy only when no Japanese one has been read.
            if candidate.rank.2 == 3 && best.is_some() {
                continue;
            }
            match self.texts(&candidate.link, &mut tries).await {
                Texts::Read(texts) => {
                    if let Some(v) = best_vn3(&texts) {
                        if better(&v, &best) {
                            best = Some((v, link_url(&candidate.link)));
                        }
                    }
                }
                Texts::Unreadable => {}
                Texts::Retry => retry = true,
            }
        }
        if let Some(holder) = holder.filter(|_| !best.as_ref().is_some_and(|(v, _)| v.complete())) {
            for free in page.free_licenses.iter().take(2) {
                match holder.download(&free.route).await {
                    Ok(bytes) => {
                        let (texts, timed_out) =
                            tokio::task::spawn_blocking(move || read_file(&bytes))
                                .await
                                .unwrap_or_default();
                        let found = best_vn3(&texts);
                        // A PDF that ran out of time may hold what this file still lacks.
                        retry |= timed_out && !found.as_ref().is_some_and(Vn3::complete);
                        if let Some(v) = found.filter(|v| better(v, &best)) {
                            best = Some((v, item_url.clone()));
                        }
                    }
                    Err(error) => {
                        // Only a file that is gone or too large is an answer. A sign-in, a
                        // refusal or a dropped connection is read again by the next sync.
                        let code = crate::booth::error_code(&error);
                        if !matches!(
                            code.as_str(),
                            "FILE_UNAVAILABLE" | "LICENSE_DOCUMENT_TOO_LARGE" | "INVALID_RESOLVER"
                        ) {
                            refused.get_or_insert(code);
                        }
                    }
                }
                if best.as_ref().is_some_and(|(v, _)| v.complete()) {
                    break;
                }
            }
        }
        // A document that could not be read may hold the license: no partial VN3, CC0 or none.
        if let Some((vn3, source)) =
            best.filter(|(v, _)| v.complete() || (refused.is_none() && !retry))
        {
            return Outcome::Found(Found {
                kind: "vn3",
                version: vn3.version,
                source,
                answers: json!(vn3.answers),
            });
        }
        if let Some(code) = refused {
            return Outcome::Failed(code);
        }
        if retry {
            return Outcome::Failed("LICENSE_DOCUMENT_FAILED".into());
        }
        if parse::cc0(&page.blocks) {
            return Outcome::Found(Found {
                kind: "cc0",
                version: None,
                source: item_url,
                answers: json!({}),
            });
        }
        Outcome::Missing
    }

    async fn texts(&self, link: &Link, tries: &mut usize) -> Texts {
        let key = format!("{link:?}");
        if let Some(texts) = self.documents.lock().await.get(&key) {
            return texts.clone();
        }
        // A folder read cut short by this asset's request budget is not the folder's answer.
        let mut cut = false;
        let texts = match link {
            Link::DriveFile { id, resource_key } => {
                *tries += 1;
                self.drive_file(id, resource_key.as_deref()).await
            }
            Link::DriveFolder { id, resource_key } => {
                *tries += 1;
                let mut url =
                    url::Url::parse("https://drive.google.com/embeddedfolderview").unwrap();
                url.query_pairs_mut().append_pair("id", id);
                if let Some(key) = resource_key {
                    url.query_pairs_mut().append_pair("resourcekey", key);
                }
                match self.google_get(url).await {
                    Ok((200, body)) => {
                        let html = String::from_utf8_lossy(&body).into_owned();
                        let mut files = folder_files(&html);
                        files.sort_by_key(|(_, name)| document_rank(name));
                        let (mut texts, mut failed, mut done) = (Texts::Unreadable, false, false);
                        for (file, _) in files.into_iter().take(2) {
                            if *tries >= TRIES {
                                cut = true;
                                break;
                            }
                            *tries += 1;
                            match self.drive_file(&file, None).await {
                                Texts::Read(read) => {
                                    done = best_vn3(&read).is_some_and(|v| v.complete());
                                    match &mut texts {
                                        Texts::Read(all) => all.extend(read),
                                        _ => texts = Texts::Read(read),
                                    }
                                    if done {
                                        break;
                                    }
                                }
                                Texts::Retry => failed = true,
                                Texts::Unreadable => {}
                            }
                        }
                        // A file that failed this time may hold the license the others lack.
                        if failed && !done {
                            Texts::Retry
                        } else {
                            texts
                        }
                    }
                    Ok((status, _)) if status == 429 || status >= 500 => Texts::Retry,
                    Ok(_) => Texts::Unreadable,
                    Err(_) => Texts::Retry,
                }
            }
            Link::Document(id) | Link::Presentation(id) => {
                *tries += 1;
                let url = match link {
                    Link::Document(_) => {
                        format!("https://docs.google.com/document/d/{id}/export?format=txt")
                    }
                    _ => format!("https://docs.google.com/presentation/d/{id}/export/txt"),
                };
                match self.google_get(url::Url::parse(&url).unwrap()).await {
                    Ok((200, body)) if !html(&body) => {
                        Texts::Read(vec![String::from_utf8_lossy(&body).into_owned()])
                    }
                    Ok((status, _)) if status == 429 || status >= 500 => Texts::Retry,
                    Ok(_) => Texts::Unreadable,
                    Err(_) => Texts::Retry,
                }
            }
        };
        if !cut && !matches!(texts, Texts::Retry) {
            self.documents.lock().await.insert(key, texts.clone());
        }
        texts
    }

    async fn drive_file(&self, id: &str, resource_key: Option<&str>) -> Texts {
        let mut url = url::Url::parse("https://drive.google.com/uc?export=download").unwrap();
        url.query_pairs_mut().append_pair("id", id);
        if let Some(key) = resource_key {
            url.query_pairs_mut().append_pair("resourcekey", key);
        }
        match self.google_get(url).await {
            // An HTML answer is a sign-in, a confirmation or "only the owner can download".
            Ok((200, body)) if !html(&body) => {
                let (texts, timed_out) = tokio::task::spawn_blocking(move || read_file(&body))
                    .await
                    .unwrap_or_default();
                // A PDF that ran out of time may still hold the license.
                if timed_out && !best_vn3(&texts).is_some_and(|v| v.complete()) {
                    Texts::Retry
                } else {
                    Texts::Read(texts)
                }
            }
            Ok((status, _)) if status == 429 || status >= 500 => Texts::Retry,
            Ok(_) => Texts::Unreadable,
            Err(_) => Texts::Retry,
        }
    }

    /// About one Google request per second for the whole job.
    async fn google_get(&self, url: url::Url) -> Result<(u16, Vec<u8>)> {
        {
            let mut next = self.pace.lock().await;
            let now = Instant::now();
            if *next > now {
                tokio::time::sleep_until(tokio::time::Instant::from_std(*next)).await;
            }
            *next = Instant::now() + Duration::from_millis(1000);
        }
        let mut response = self.google.get(url).send().await?;
        let status = response.status();
        if !google_host(response.url()) || status.is_redirection() {
            return Ok((StatusCode::FORBIDDEN.as_u16(), vec![]));
        }
        if response
            .content_length()
            .is_some_and(|n| n > DOCUMENT_LIMIT)
        {
            return Ok((StatusCode::PAYLOAD_TOO_LARGE.as_u16(), vec![]));
        }
        let mut body = vec![];
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > DOCUMENT_LIMIT as usize {
                return Ok((StatusCode::PAYLOAD_TOO_LARGE.as_u16(), vec![]));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status.as_u16(), body))
    }
}

fn html(body: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&body[..body.len().min(2000)]).to_lowercase();
    head.contains("<html") || head.contains("<!doctype html")
}

fn link_url(link: &Link) -> String {
    // Older link-shared Drive items open only with their resource key.
    let keyed = |base: String, key: &Option<String>| match (url::Url::parse(&base), key) {
        (Ok(mut url), Some(key)) => {
            url.query_pairs_mut().append_pair("resourcekey", key);
            url.to_string()
        }
        _ => base,
    };
    match link {
        Link::DriveFile { id, resource_key } => keyed(
            format!("https://drive.google.com/file/d/{id}/view"),
            resource_key,
        ),
        Link::DriveFolder { id, resource_key } => keyed(
            format!("https://drive.google.com/drive/folders/{id}"),
            resource_key,
        ),
        Link::Document(id) => format!("https://docs.google.com/document/d/{id}"),
        Link::Presentation(id) => format!("https://docs.google.com/presentation/d/{id}"),
    }
}

fn folder_files(html: &str) -> Vec<(String, String)> {
    let document = scraper::Html::parse_document(html);
    let entry = scraper::Selector::parse("div.flip-entry").unwrap();
    let title = scraper::Selector::parse("div.flip-entry-title").unwrap();
    document
        .select(&entry)
        .filter_map(|e| {
            let id = e.value().id()?.strip_prefix("entry-")?.to_owned();
            let name = e
                .select(&title)
                .next()?
                .text()
                .collect::<String>()
                .trim()
                .to_owned();
            let lower = name.to_lowercase();
            (lower.ends_with(".pdf") || lower.ends_with(".zip")).then_some((id, name))
        })
        .collect()
}

/// The most complete document; on a tie the first, which the page ranked higher.
fn best_vn3(texts: &[String]) -> Option<Vn3> {
    texts
        .iter()
        .filter_map(|text| parse::vn3(text))
        .fold(None, |best, v| match best {
            Some(b) if (b.complete(), b.answers.len()) >= (v.complete(), v.answers.len()) => {
                Some(b)
            }
            _ => Some(v),
        })
}

/// Japanese and license-named documents first, other languages last.
fn document_rank(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if lower.contains("_ja.")
        || lower.contains("jp")
        || name.contains("日本語")
        || name.contains("規約")
    {
        0
    } else if parse::other_language(name) {
        2
    } else {
        1
    }
}

/// Text of a PDF, a plain-text file, or the PDFs and text files inside a ZIP.
pub fn file_texts(bytes: &[u8]) -> Vec<String> {
    read_file(bytes).0
}

/// The texts, and whether a PDF ran out of time: then the file is worth another try.
fn read_file(bytes: &[u8]) -> (Vec<String>, bool) {
    use std::io::Read;
    if bytes.starts_with(b"%PDF-") {
        return match pdf_text(bytes.to_vec()) {
            Pdf::Text(text) => (vec![text], false),
            Pdf::Unreadable => (vec![], false),
            Pdf::TimedOut => (vec![], true),
        };
    }
    if bytes.starts_with(b"PK\x03\x04") {
        let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
            return (vec![], false);
        };
        let mut names: Vec<(usize, String)> = (0..archive.len().min(200))
            .filter_map(|index| Some((index, archive.by_index(index).ok()?.name().to_owned())))
            .collect();
        names.sort_by_key(|(index, name)| (document_rank(name), *index));
        let (mut texts, mut pdfs, mut timed_out) = (vec![], 0, false);
        for (index, name) in names {
            let Ok(mut entry) = archive.by_index(index) else {
                continue;
            };
            let lower = name.to_lowercase();
            let pdf = lower.ends_with(".pdf");
            // A few PDFs at most: each may take up to the parser's whole time limit.
            if !(pdf || lower.ends_with(".txt"))
                || entry.size() > DOCUMENT_LIMIT
                || texts.len() >= 8
                || (pdf && pdfs >= 4)
            {
                continue;
            }
            pdfs += usize::from(pdf);
            let mut data = vec![];
            if entry
                .by_ref()
                .take(DOCUMENT_LIMIT)
                .read_to_end(&mut data)
                .is_err()
            {
                continue;
            }
            if pdf {
                match pdf_text(data) {
                    Pdf::Text(text) => texts.push(text),
                    Pdf::Unreadable => {}
                    Pdf::TimedOut => timed_out = true,
                }
            } else if let Ok(text) = String::from_utf8(data) {
                texts.push(text.trim_start_matches('\u{feff}').to_owned());
            }
        }
        return (texts, timed_out);
    }
    if html(bytes) {
        return (vec![], false);
    }
    let texts = std::str::from_utf8(bytes)
        .map(|text| vec![text.trim_start_matches('\u{feff}').to_owned()])
        .unwrap_or_default();
    (texts, false)
}

enum Pdf {
    Text(String),
    Unreadable,
    TimedOut,
}

/// PDF text on its own thread: a malformed file can panic inside the parser or take too
/// long. Nested objects recurse in the parser, so the thread gets a large stack; generator
/// PDFs take well under a second.
fn pdf_text(bytes: Vec<u8>) -> Pdf {
    let (sender, receiver) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let text = std::panic::catch_unwind(|| {
                let document = lopdf::Document::load_mem(&bytes).ok()?;
                let pages: Vec<u32> = document.get_pages().keys().copied().collect();
                document.extract_text(&pages).ok()
            })
            .ok()
            .flatten();
            let _ = sender.send(text);
        });
    if spawned.is_err() {
        return Pdf::Unreadable;
    }
    match receiver.recv_timeout(Duration::from_secs(20)) {
        Ok(Some(text)) => Pdf::Text(text),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Pdf::TimedOut,
        _ => Pdf::Unreadable,
    }
}
