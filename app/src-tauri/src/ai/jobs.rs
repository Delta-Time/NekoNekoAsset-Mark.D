//! Sequential, cancellable AI jobs. Restart never resubmits a paid request.
use crate::{
    ai::analysis::{self, AnalysisInput, FileEvidence, TagAliasSuggestion},
    ai::{AiConfig, AiImage, AiRequest, AiService},
    catalogue::Catalogue,
    preview::{self, PreviewCache},
    storage::StorageService,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeSet, HashMap},
    fs::File,
    io::Read,
    path::Path,
    sync::Arc,
};
use tauri::{Emitter, Listener};
use tokio::sync::{watch, Mutex};

/// What an AI analysis may send to the provider.
#[derive(Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiScopes {
    pub title: bool,
    pub tags: bool,
    pub description: bool,
    pub files: bool,
    pub images: bool,
}
impl Default for AiScopes {
    fn default() -> Self {
        Self {
            title: true,
            tags: true,
            description: true,
            files: true,
            images: false,
        }
    }
}
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    Classify,
    Organize,
}
/// One AI analysis action.
#[derive(Deserialize, ts_rs::TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
#[ts(export)]
pub enum AiJobRequest {
    Start {
        mode: AiMode,
        #[serde(rename = "productIds")]
        product_ids: Vec<String>,
        scopes: AiScopes,
    },
    Cancel {
        #[serde(rename = "jobId")]
        job_id: String,
    },
    Retry {
        #[serde(rename = "jobId")]
        job_id: String,
    },
    DismissSourceUrl {
        #[serde(rename = "productId")]
        product_id: String,
        url: String,
    },
}
/// The job an analysis action started or touched.
#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiJobStarted {
    pub job_id: String,
}

#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AiDone {
    pub ok: bool,
}

/// The answer to each AI analysis action, for the typed AI calls in the UI.
#[cfg(test)]
#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
struct AiJobResponses {
    start: AiJobStarted,
    cancel: AiJobStarted,
    retry: AiJobStarted,
    dismiss_source_url: AiDone,
}

pub struct AiJobs {
    catalogue: Catalogue,
    ai: Arc<AiService>,
    storage: Arc<StorageService>,
    previews: Arc<PreviewCache>,
    queue: Mutex<()>,
    enqueue_gate: Mutex<()>,
    cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
    known_products: Mutex<BTreeSet<String>>,
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn safe_error(error: anyhow::Error) -> String {
    let code = error.to_string().split(':').next().unwrap_or("").to_owned();
    if code.len() <= 80
        && (code.starts_with("ai_") || code.starts_with("AI_"))
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        code
    } else {
        "ai_unavailable".into()
    }
}
fn selected_input(mut input: AnalysisInput, scopes: &AiScopes) -> Result<AnalysisInput> {
    ensure!(
        scopes.title || scopes.tags || scopes.description || scopes.files || scopes.images,
        "ai_scope_empty"
    );
    if !scopes.title {
        input.title.clear();
    }
    if !scopes.tags {
        input.source_tags.clear();
        input.rejected_tags.clear();
    }
    if !scopes.description {
        input.description.clear();
    }
    if !scopes.files {
        input.files.clear();
    }
    analysis::normalize_input(input)
}

fn read_preview(path: &Path) -> Result<AiImage> {
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    preview::no_links(path).map_err(|_| anyhow::anyhow!("ai_image_invalid"))?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| anyhow::anyhow!("ai_image_missing"))?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_BYTES,
        "ai_image_invalid"
    );
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| anyhow::anyhow!("ai_image_missing"))?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("ai_image_invalid"))?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "ai_image_invalid");
    let mime = match image::guess_format(&bytes).ok() {
        Some(image::ImageFormat::Png) => "image/png",
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        Some(image::ImageFormat::WebP) => "image/webp",
        Some(image::ImageFormat::Gif) => "image/gif",
        _ => anyhow::bail!("ai_image_invalid"),
    };
    AiImage::from_bytes(mime, &bytes)
}

fn add_preview_evidence(input: &mut AnalysisInput, image: &AiImage) -> Result<()> {
    input.files.insert(
        0,
        FileEvidence {
            evidence_ref: image.evidence_ref(),
            file_name: if image.mime_type == "image/png" {
                "preview.png"
            } else {
                "preview.jpg"
            }
            .into(),
            text_excerpt: String::new(),
            candidate_public_urls: vec![],
        },
    );
    while input.files.len() > 64 || serde_json::to_vec(input)?.len() > 128 * 1024 {
        ensure!(input.files.len() > 1, "AI_INVALID_ANALYSIS");
        input.files.pop();
    }
    *input = analysis::normalize_input(input.clone())?;
    Ok(())
}

fn enqueue_selection(
    ids: &[String],
    jobs: &[Value],
    mode: AiMode,
    scopes: &AiScopes,
    config: &AiConfig,
) -> (Vec<String>, Option<String>) {
    let mode_value = serde_json::to_value(mode).expect("AI mode serializes");
    let scopes_value = serde_json::to_value(scopes).expect("AI scopes serialize");
    let config_value = serde_json::to_value(config).expect("AI config serializes");
    let requested: BTreeSet<_> = ids.iter().cloned().collect();
    let mut active = BTreeSet::new();
    let mut first_job = None;
    for job in jobs {
        if !job["type"]
            .as_str()
            .is_some_and(|kind| kind.starts_with("ai:"))
            || !matches!(job["state"].as_str(), Some("queued" | "running"))
            || job["aiConfig"] != config_value
            || job["aiMode"] != mode_value
            || mode == AiMode::Classify && job["aiScopes"] != scopes_value
        {
            continue;
        }
        let Some(id) = job["id"].as_str().filter(|id| !id.is_empty()) else {
            continue;
        };
        let covered: BTreeSet<String> =
            serde_json::from_value(job["productIds"].clone()).unwrap_or_default();
        if requested.is_subset(&covered) {
            return (vec![], Some(id.to_owned()));
        }
        if !requested.is_disjoint(&covered) && first_job.is_none() {
            first_job = Some(id.to_owned());
        }
        active.extend(covered);
    }
    let remaining: Vec<_> = requested.difference(&active).cloned().collect();
    if remaining.is_empty() {
        (remaining, first_job)
    } else {
        (remaining, None)
    }
}
impl AiJobs {
    pub fn new(
        catalogue: Catalogue,
        ai: Arc<AiService>,
        storage: Arc<StorageService>,
        previews: Arc<PreviewCache>,
    ) -> Self {
        Self {
            catalogue,
            ai,
            storage,
            previews,
            queue: Mutex::new(()),
            enqueue_gate: Mutex::new(()),
            cancellations: Mutex::new(HashMap::new()),
            known_products: Mutex::new(BTreeSet::new()),
        }
    }
    async fn selected_preview(
        &self,
        product_id: &str,
        fingerprint: &str,
    ) -> Result<Option<AiImage>> {
        let source = self
            .catalogue
            .ai_preview_source(product_id)
            .map_err(|_| anyhow::anyhow!("ai_image_invalid"))?;
        ensure!(source.preview_fingerprint == fingerprint, "AI_INPUT_STALE");
        let path = if let Some(relative) = source.managed_relative_path {
            self.catalogue.data_dir().join(relative)
        } else if let Some(url) = source.public_url {
            self.previews
                .get(url)
                .await
                .map_err(|_| anyhow::anyhow!("ai_image_unavailable"))?
        } else {
            return Ok(None);
        };
        tokio::task::spawn_blocking(move || read_preview(&path))
            .await
            .map_err(|_| anyhow::anyhow!("ai_image_invalid"))?
            .map(Some)
    }
    fn notify(&self, app: &tauri::AppHandle, scope: &str, ids: &[String]) {
        let _ = app.emit("nna:changed", crate::ipc::Change::products(scope, ids));
    }
    fn save_job(
        &self,
        app: &tauri::AppHandle,
        job: &mut Value,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        job["state"] = json!(state);
        job["errorCode"] = json!(error);
        job["updatedAt"] = json!(now());
        self.catalogue
            .put("job", job["id"].as_str().unwrap_or(""), job.clone())?;
        self.notify(app, "jobs", &[]);
        Ok(())
    }
    pub async fn start(self: &Arc<Self>, app: &tauri::AppHandle) -> Result<()> {
        let _work = self.catalogue.begin_catalogue_work()?;
        *self.known_products.lock().await = self.catalogue.product_ids()?.into_iter().collect();
        for mut job in self.catalogue.list("job")? {
            if job["type"].as_str().is_some_and(|t| t.starts_with("ai:"))
                && matches!(job["state"].as_str(), Some("queued" | "running"))
            {
                self.save_job(app, &mut job, "cancelled", Some("ai_interrupted"))?;
            }
        }
        let service = self.clone();
        let handle = app.clone();
        app.listen("nna:changed", move |event| {
            let scope = serde_json::from_str::<crate::ipc::Change>(event.payload())
                .ok()
                .map(|change| change.scope);
            if matches!(
                scope.as_deref(),
                Some("products" | "catalogue" | "booth" | "storage")
            ) {
                let service = service.clone();
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = service.automatic(&handle).await;
                });
            }
        });
        Ok(())
    }
    pub async fn reset_automatic_baseline(&self) -> Result<()> {
        *self.known_products.lock().await = self.catalogue.product_ids()?.into_iter().collect();
        Ok(())
    }
    pub async fn cancel_all(self: &Arc<Self>, app: &tauri::AppHandle) -> Result<()> {
        let jobs = {
            let _gate = self.enqueue_gate.lock().await;
            self.catalogue.list("job")?
        };
        for job in jobs {
            if job["type"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("ai:"))
                && matches!(job["state"].as_str(), Some("queued" | "running"))
            {
                if let Some(id) = job["id"].as_str() {
                    self.command(
                        app,
                        AiJobRequest::Cancel {
                            job_id: id.to_owned(),
                        },
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
    async fn automatic(self: &Arc<Self>, app: &tauri::AppHandle) -> Result<()> {
        let mut known = self.known_products.lock().await;
        let current: BTreeSet<_> = self.catalogue.product_ids()?.into_iter().collect();
        let ids: Vec<_> = current.difference(&known).cloned().collect();
        let preferences = self
            .catalogue
            .get("preferences", "app")?
            .unwrap_or(json!({}));
        if preferences["aiFeaturesEnabled"] != true
            || preferences["aiEnabled"] != true
            || ids.is_empty()
        {
            *known = current;
            return Ok(());
        }
        known.retain(|id| current.contains(id));
        let scopes: AiScopes =
            serde_json::from_value(preferences["aiScopes"].clone()).unwrap_or_default();
        for batch in ids.chunks(250) {
            self.enqueue_inner(app, AiMode::Classify, batch.to_vec(), scopes.clone(), true)
                .await?;
            known.extend(batch.iter().cloned());
        }
        Ok(())
    }
    pub async fn command(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        request: AiJobRequest,
    ) -> Result<Value> {
        let _work = self.catalogue.begin_catalogue_work()?;
        match request {
            AiJobRequest::Start {
                mode,
                product_ids,
                scopes,
            } => self.enqueue(app, mode, product_ids, scopes).await,
            AiJobRequest::Cancel { job_id } => {
                let _gate = self.enqueue_gate.lock().await;
                let mut job = self
                    .catalogue
                    .get("job", &job_id)?
                    .ok_or_else(|| anyhow::anyhow!("ai_job_missing"))?;
                ensure!(
                    job["type"].as_str().is_some_and(|t| t.starts_with("ai:")),
                    "ai_job_missing"
                );
                if matches!(job["state"].as_str(), Some("queued" | "running")) {
                    if let Some(sender) = self.cancellations.lock().await.get(&job_id) {
                        let _ = sender.send(true);
                    }
                    self.save_job(app, &mut job, "cancelled", None)?;
                }
                Ok(serde_json::to_value(AiJobStarted { job_id })?)
            }
            AiJobRequest::Retry { job_id } => {
                let job = self
                    .catalogue
                    .get("job", &job_id)?
                    .ok_or_else(|| anyhow::anyhow!("ai_job_missing"))?;
                ensure!(
                    job["type"].as_str().is_some_and(|t| t.starts_with("ai:"))
                        && matches!(
                            job["state"].as_str(),
                            Some("failed" | "cancelled" | "needsAction")
                        ),
                    "ai_job_missing"
                );
                let completed: BTreeSet<String> =
                    serde_json::from_value(job["completedProductIds"].clone()).unwrap_or_default();
                let ids: Vec<String> = serde_json::from_value(job["productIds"].clone())?;
                let mut remaining = vec![];
                for id in ids {
                    if !completed.contains(&id) && self.catalogue.get("product", &id)?.is_some() {
                        remaining.push(id);
                    }
                }
                // Empty is a global-organize intent only for an explicit Start,
                // never for a fully applied cancelled/failed batch's Retry.
                if remaining.is_empty() {
                    return Ok(serde_json::to_value(AiJobStarted { job_id })?);
                }
                self.enqueue(
                    app,
                    serde_json::from_value(job["aiMode"].clone())?,
                    remaining,
                    serde_json::from_value(job["aiScopes"].clone())?,
                )
                .await
            }
            AiJobRequest::DismissSourceUrl { product_id, url } => {
                self.catalogue
                    .dismiss_ai_source_url_candidate(&product_id, &url)?;
                self.notify(app, "products", &[product_id]);
                Ok(serde_json::to_value(AiDone { ok: true })?)
            }
        }
    }
    async fn enqueue(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        mode: AiMode,
        ids: Vec<String>,
        scopes: AiScopes,
    ) -> Result<Value> {
        self.enqueue_inner(app, mode, ids, scopes, false).await
    }
    async fn enqueue_inner(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        mode: AiMode,
        ids: Vec<String>,
        scopes: AiScopes,
        allow_missing_key: bool,
    ) -> Result<Value> {
        let _gate = self.enqueue_gate.lock().await;
        let work = self.catalogue.begin_catalogue_work()?;
        ensure!(
            self.catalogue
                .get("preferences", "app")?
                .is_some_and(|p| p["aiFeaturesEnabled"] == true),
            "ai_features_disabled"
        );
        if mode == AiMode::Classify {
            selected_input(
                AnalysisInput {
                    title: String::new(),
                    source_tags: vec![],
                    description: String::new(),
                    rejected_tags: vec![],
                    files: vec![],
                },
                &scopes,
            )?;
        }
        let status = serde_json::to_value(self.ai.command(AiRequest::KeyStatus).await?)?;
        // Automatic work needs a recoverable Activity entry if a saved key was removed.
        ensure!(
            allow_missing_key || status["keyPresent"] == true || status["keyRequired"] == false,
            "ai_key_missing"
        );
        let config = self.ai.current_config().await?;
        ensure!(
            mode != AiMode::Classify || !scopes.images || config.vision_supported(),
            "ai_images_unavailable"
        );
        let ids: Vec<_> = if mode == AiMode::Organize && ids.is_empty() {
            self.catalogue.product_ids()?
        } else {
            ids.into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        ensure!(
            !ids.is_empty() && ids.len() <= 10000,
            "ai_selection_invalid"
        );
        for id in &ids {
            ensure!(
                self.catalogue.get("product", id)?.is_some(),
                "ai_selection_invalid"
            );
        }
        let (ids, covering_job) =
            enqueue_selection(&ids, &self.catalogue.list("job")?, mode, &scopes, &config);
        if let Some(id) = covering_job {
            return Ok(serde_json::to_value(AiJobStarted { job_id: id })?);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = watch::channel(false);
        self.cancellations.lock().await.insert(id.clone(), sender);
        let mut job = json!({"id":id,"type":if mode==AiMode::Organize{"ai:organize"}else{"ai:classify"},"aiMode":mode,"aiScopes":scopes,"aiConfig":config,"productIds":ids,"completedProductIds":[],"failures":{},"progress":0,"createdAt":now()});
        self.save_job(app, &mut job, "queued", None)?;
        let service = self.clone();
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let _work = work;
            service.run(&handle, job, mode, scopes, receiver).await;
        });
        Ok(serde_json::to_value(AiJobStarted { job_id: id })?)
    }
    async fn run(
        self: Arc<Self>,
        app: &tauri::AppHandle,
        mut job: Value,
        mode: AiMode,
        scopes: AiScopes,
        mut cancellation: watch::Receiver<bool>,
    ) {
        let _serial = self.queue.lock().await;
        let id = job["id"].as_str().unwrap_or("").to_owned();
        let result: Result<()> = async {
            let config: AiConfig = serde_json::from_value(job["aiConfig"].clone())?;
            {
                let _gate = self.enqueue_gate.lock().await;
                ensure!(!*cancellation.borrow(), "ai_cancelled");
                ensure!(self.catalogue.get("preferences", "app")?.is_some_and(|p| p["aiFeaturesEnabled"] == true), "ai_features_disabled");
                self.save_job(app, &mut job, "running", None)?;
            }
            if mode == AiMode::Organize {
                self.organize(app, &mut job, &mut cancellation, &config).await?;
            } else {
                let ids: Vec<String> = serde_json::from_value(job["productIds"].clone())?;
                for (index, product_id) in ids.iter().enumerate() {
                    ensure!(!*cancellation.borrow(), "ai_cancelled");
                    ensure!(self.catalogue.get("preferences", "app")?.is_some_and(|p| p["aiFeaturesEnabled"] == true), "ai_features_disabled");
                    let prepared = async {
                    let snapshot = self.catalogue.collect_ai_metadata(product_id)?;
                    let mut input = selected_input(snapshot.input, &scopes)?;
                    if scopes.files {
                        input = self.storage.collect_ai_evidence(product_id, input).await?;
                    }
                    if scopes.files
                        && self
                            .catalogue
                            .get("product", product_id)?
                            .is_some_and(|p| p["provider"] == "local")
                    {
                        let candidates = self.catalogue.ai_source_candidates(&input)?;
                        input.files.sort_by_key(|file| file.text_excerpt.is_empty());
                        input
                            .files
                            .truncate(64usize.saturating_sub(candidates.len()));
                        input.files.extend(candidates);
                        while serde_json::to_vec(&input)?.len() > 128 * 1024 {
                            input.files.pop();
                        }
                        input = analysis::normalize_input(input)?;
                    }
                    Ok::<_, anyhow::Error>((snapshot.catalogue_fingerprint, snapshot.preview_fingerprint, input))
                    }.await;
                    let (catalogue_fingerprint, preview_fingerprint, mut input) = match prepared {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            let _gate = self.enqueue_gate.lock().await;
                            ensure!(!*cancellation.borrow(), "ai_cancelled");
                            ensure!(self.catalogue.get("preferences", "app")?.is_some_and(|p| p["aiFeaturesEnabled"] == true), "ai_features_disabled");
                            job["failures"][product_id] = json!(safe_error(error));
                            job["progress"] = json!((index + 1) * 100 / ids.len());
                            self.save_job(app, &mut job, "running", None)?;
                            continue;
                        }
                    };
                    // Cancellation during document reads must not start a paid
                    // request; if both futures are ready, cancellation wins.
                    ensure!(!*cancellation.borrow(), "ai_cancelled");
                    let request_cancellation = cancellation.clone();
                    let output = tokio::select! {
                        biased;
                        _ = cancellation.changed() => Err(anyhow::anyhow!("ai_cancelled")),
                        result = async {
                            let image = if scopes.images {
                                self.selected_preview(product_id, &preview_fingerprint).await?
                            } else { None };
                            ensure!(image.is_some() || !input.title.trim().is_empty() || !input.source_tags.is_empty() || !input.description.trim().is_empty() || !input.files.is_empty(), if scopes.images { "ai_image_missing" } else { "ai_scope_empty" });
                            if let Some(image) = &image {
                                add_preview_evidence(&mut input, image)?;
                            }
                            ensure!(!*request_cancellation.borrow(), "ai_cancelled");
                            ensure!(self.catalogue.get("preferences", "app")?.is_some_and(|p| p["aiFeaturesEnabled"] == true), "ai_features_disabled");
                            self.ai.analyze_with_images_for_config(&input, image.as_slice(), &config).await
                                .map(|output| (output, image.is_some()))
                        } => result,
                    };
                    let _gate = self.enqueue_gate.lock().await;
                    ensure!(!*cancellation.borrow(), "ai_cancelled");
                    ensure!(self.catalogue.get("preferences", "app")?.is_some_and(|p| p["aiFeaturesEnabled"] == true), "ai_features_disabled");
                    match output.and_then(|(output, used_preview)| {
                        self.catalogue.publish_ai_analysis_with_preview(
                            product_id,
                            &catalogue_fingerprint,
                            &input,
                            &output,
                            &config.model,
                            used_preview.then_some(preview_fingerprint.as_str()),
                        )
                    }) {
                        Ok(()) => {
                            job["completedProductIds"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!(product_id));
                            self.notify(app, "products", &[product_id.clone()]);
                        }
                        Err(error) => {
                            let code = safe_error(error);
                            if code == "ai_cancelled" {
                                anyhow::bail!("ai_cancelled");
                            }
                            job["failures"][product_id] = json!(code);
                            if matches!(
                                code.as_str(),
                                "ai_auth_failed"
                                    | "ai_quota_exceeded"
                                    | "ai_spend_limit_exceeded"
                                    | "ai_rate_limited"
                                    | "ai_key_missing"
                                    | "ai_config_changed"
                            ) {
                                anyhow::bail!(code);
                            }
                        }
                    }
                    job["progress"] = json!((index + 1) * 100 / ids.len());
                    self.save_job(app, &mut job, "running", None)?;
                }
            }
            Ok(())
        }
        .await;
        let _gate = self.enqueue_gate.lock().await;
        let result = if *cancellation.borrow() {
            Err(anyhow::anyhow!("ai_cancelled"))
        } else {
            result
        };
        match result {
            Ok(()) => {
                let failed = job["failures"].as_object().is_some_and(|v| !v.is_empty());
                let _ = self.save_job(
                    app,
                    &mut job,
                    if failed { "needsAction" } else { "completed" },
                    if failed {
                        Some("ai_partial_failure")
                    } else {
                        None
                    },
                );
            }
            Err(error) => {
                let code = safe_error(error);
                let _ = self.save_job(
                    app,
                    &mut job,
                    if code == "ai_cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    Some(&code),
                );
            }
        }
        drop(_gate);
        self.cancellations.lock().await.remove(&id);
    }
    async fn organize(
        &self,
        app: &tauri::AppHandle,
        job: &mut Value,
        cancellation: &mut watch::Receiver<bool>,
        config: &AiConfig,
    ) -> Result<()> {
        let ids: Vec<String> = serde_json::from_value(job["productIds"].clone())?;
        let mut snapshots = vec![];
        let mut tags = BTreeSet::new();
        for id in &ids {
            ensure!(!*cancellation.borrow(), "ai_cancelled");
            let snapshot = match self.catalogue.collect_ai_metadata(id) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    job["failures"][id] = json!(safe_error(error));
                    continue;
                }
            };
            tags.extend(snapshot.input.source_tags.iter().cloned());
            snapshots.push((id, snapshot));
        }
        ensure!(tags.len() <= 4096, "ai_tag_limit");
        let tags: Vec<_> = tags.into_iter().collect();
        let mut aliases = vec![];
        for batch in tags.chunks(128) {
            ensure!(
                self.catalogue
                    .get("preferences", "app")?
                    .is_some_and(|p| p["aiFeaturesEnabled"] == true),
                "ai_features_disabled"
            );
            if *cancellation.borrow() {
                anyhow::bail!("ai_cancelled");
            }
            let input = analysis::normalize_input(AnalysisInput {
                title: String::new(),
                source_tags: batch.to_vec(),
                description: String::new(),
                rejected_tags: vec![],
                files: vec![],
            })?;
            let output = tokio::select! {
                biased;
                _ = cancellation.changed() => { anyhow::bail!("ai_cancelled") },
                result = self.ai.organize_for_config(&input, config) => result?,
            };
            aliases.extend(
                output
                    .tag_aliases
                    .into_iter()
                    .filter(|alias| alias.confidence >= 0.85 && batch.contains(&alias.from)),
            );
        }
        for (index, (id, snapshot)) in snapshots.iter().enumerate() {
            let _gate = self.enqueue_gate.lock().await;
            ensure!(!*cancellation.borrow(), "ai_cancelled");
            ensure!(
                self.catalogue
                    .get("preferences", "app")?
                    .is_some_and(|p| p["aiFeaturesEnabled"] == true),
                "ai_features_disabled"
            );
            let relevant: Vec<TagAliasSuggestion> = aliases
                .iter()
                .filter(|a| snapshot.input.source_tags.contains(&a.from))
                .cloned()
                .collect();
            match self.catalogue.publish_ai_tag_organization(
                id,
                &snapshot.catalogue_fingerprint,
                &relevant,
                &config.model,
            ) {
                Ok(()) => {
                    job["completedProductIds"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(id));
                    self.notify(app, "products", &[(*id).clone()]);
                }
                Err(error) => job["failures"][*id] = json!(safe_error(error)),
            }
            job["progress"] = json!((index + 1) * 100 / ids.len());
            self.save_job(app, job, "running", None)?;
        }
        job["progress"] = json!(100);
        Ok(())
    }
}
#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
