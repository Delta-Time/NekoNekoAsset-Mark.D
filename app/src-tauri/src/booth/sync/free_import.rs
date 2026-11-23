//! Adding free BOOTH items by their listing. The account reads each item's own page, as a
//! free item's Update does, and takes what that page offers for free as the account's free
//! download; a paid variation offers no download there, so it is never touched. The row it
//! publishes has the record ID BOOTH's free history gives the item, so the history a later
//! sync reads merges with it, and the time it was added, which that history does not show.
//! Each added item then downloads like one the user asked to download.
use super::*;

/// How many listings one import reads at most.
pub(in crate::booth) const FREE_IMPORT_LIMIT: usize = 500;

/// What one import has done so far.
#[derive(Default)]
struct FreeImport {
    enriched: BTreeMap<String, Value>,
    publications: Publications,
    added: Vec<String>,
    /// Listings left out, with the reason and the title when the listing gave one.
    skipped: serde_json::Map<String, Value>,
    last_added_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl FreeImport {
    /// When an item is added: now, or just after the item before it, so items added in one
    /// import never share a time and keep the order they were added in.
    fn next_time(&mut self) -> chrono::DateTime<chrono::Utc> {
        let now = chrono::Utc::now();
        let at = match self.last_added_at {
            Some(last) if now <= last => last + chrono::Duration::milliseconds(1),
            _ => now,
        };
        self.last_added_at = Some(at);
        at
    }
}

/// The listings an import names, in the order given, without repeats.
pub(in crate::booth) fn free_import_ids(args: &Value) -> Result<Vec<String>> {
    let mut ids: Vec<String> = Vec::new();
    for value in args["productIds"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?
    {
        let id = value
            .as_str()
            .filter(|id| safety::numeric(id))
            .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?;
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_owned());
        }
    }
    ensure!(
        !ids.is_empty() && ids.len() <= FREE_IMPORT_LIMIT,
        "INVALID_PRODUCT_IDS"
    );
    Ok(ids)
}

/// Why a free item page gives nothing to add.
fn unreadable_reason(evidence: Option<&free_item::Evidence>) -> &'static str {
    match evidence.and_then(|evidence| evidence.error_code.as_deref()) {
        Some("FREE_OFFER_UNAVAILABLE") => "FREE_IMPORT_NOT_FREE",
        Some("PUBLIC_METADATA_UNAVAILABLE") => "PUBLIC_METADATA_UNAVAILABLE",
        _ => "FREE_IMPORT_UNREADABLE",
    }
}

impl BoothService {
    /// Queues one import of these listings for the account, or answers the same import
    /// already waiting or running. It runs as an Update of the account, so it waits for the
    /// account's sync and resumes after a restart like one.
    pub(in crate::booth) fn import_free(&self, app: &AppHandle, args: &Value) -> Result<Value> {
        let account = field(args, "accountId")?.to_owned();
        self.account(&account)?;
        let ids = free_import_ids(args)?;
        let scope = json!({"accountId":account,"productIds":ids,"freeImport":true});
        if let Some(active) = self.active_update(&scope)? {
            return crate::stores::api::StoreJob::of(&active);
        }
        let job = self.make_job("boothUpdate", scope)?;
        self.spawn_job(app.clone(), job.clone())?;
        crate::stores::api::StoreJob::of(&job)
    }

    /// Adds each listing in order. One it cannot add is named in the job's `failures` with
    /// the reason; the ones added are `addedProductIds`.
    pub(super) async fn free_import_job(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        let account = field(job, "accountId")?.to_owned();
        let ids = free_import_ids(&job["args"])?;
        let client = self.client(app, &account, false).await?;
        let public =
            http::build_client(std::sync::Arc::new(reqwest::cookie::Jar::default()), false)?;
        // Item pages are public, so the history's first page is what tells a lapsed session:
        // the job stops there and asks for sign-in, as a sync would.
        let head = format!("{}?page=1", safety::source_path("free")?);
        self.read_page(&client, "free", &head, None).await?;
        // Terms of use that the list hides are not worth a download either.
        let hide_terms = self
            .inner
            .catalogue
            .get("preferences", "app")?
            .is_none_or(|preferences| preferences["hideTermsOnly"] != false);
        let mut import = FreeImport::default();
        for (index, product) in ids.iter().enumerate() {
            if self.cancelled(job)? {
                bail!("CANCELLED");
            }
            job["progress"] = json!({"stage":"collecting","mode":"selected","detailsCompleted":index,"published":import.added.len()});
            self.progress(app, job)?;
            let reason = self
                .add_free_item(
                    app,
                    job,
                    &account,
                    &client,
                    &public,
                    product,
                    hide_terms,
                    &mut import,
                )
                .await?;
            match reason {
                None => import.added.push(product.clone()),
                Some((code, title)) => {
                    let mut skipped = json!({"errorCode":code});
                    if let Some(title) = title {
                        skipped["title"] = json!(title);
                    }
                    import.skipped.insert(product.clone(), skipped);
                }
            }
            job["addedProductIds"] = json!(import.added);
            job["failures"] = Value::Object(import.skipped.clone());
        }
        // Keep the last listing's answer even if the final commit fails.
        self.progress(app, job)?;
        self.publish_collected(app, job, &mut import.publications)
            .await?;
        job["progress"] = json!({"stage":"finalizing","mode":"selected","detailsCompleted":ids.len(),"published":import.added.len()});
        self.progress(app, job)
    }

    /// Adds one listing, or answers why not and the listing's title when it gave one.
    async fn add_free_item(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        client: &reqwest::Client,
        public: &reqwest::Client,
        product: &str,
        hide_terms: bool,
        import: &mut FreeImport,
    ) -> Result<Option<(&'static str, Option<String>)>> {
        // An item the account holds already, from an earlier import or its own history, only
        // needs its files downloaded; Update is the way to read its page again.
        let held = self
            .inner
            .catalogue
            .related("acquisition", product)?
            .iter()
            .chain(self.inner.catalogue.related("entitlement", product)?.iter())
            .any(|record| record["accountId"] == account);
        if held {
            // Terms of use the list leaves out are not worth a download here either.
            if hide_terms && self.inner.catalogue.terms_only(product)? {
                let title = self
                    .inner
                    .catalogue
                    .get("product", product)?
                    .and_then(|listing| listing["sourceTitle"].as_str().map(str::to_owned));
                return Ok(Some(("FREE_TERMS_ONLY", title)));
            }
            self.want_download(product)?;
            source_changed(app, std::iter::once(product.to_owned()));
            return Ok(None);
        }
        // The listing first: its title names a listing left out, and its variations say
        // whether anything is free and whether the free part is only the terms.
        let mut draft = self
            .inner
            .catalogue
            .get("product", product)?
            .unwrap_or_else(|| json!({"id":product}));
        match self.enrich_into(public, product, &mut draft, None).await {
            Ok(()) => {}
            Err(error) if product_local_failure(&crate::booth::code(&error)) => {
                return Ok(Some(("FREE_IMPORT_UNREADABLE", None)));
            }
            Err(error) => return Err(error),
        }
        let title = draft["sourceTitle"].as_str().map(str::to_owned);
        if draft["publicPageAvailability"] == "unavailable" {
            return Ok(Some(("PUBLIC_METADATA_UNAVAILABLE", title)));
        }
        let variants = draft["publicVariants"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !variants.is_empty()
            && variants
                .iter()
                .all(|variant| variant["price"].as_f64().is_some_and(|price| price > 0.0))
        {
            return Ok(Some(("FREE_IMPORT_NOT_FREE", title)));
        }
        let route = safety::free_item_route(product)?;
        let page = self.fetch_page(client, "free", &route, None).await?;
        let Some(evidence) = page.free_item.as_ref().filter(|evidence| {
            evidence.product_id == product
                && evidence.manifest_complete
                && !evidence.unavailable
                && evidence.error_code.is_none()
                && !evidence.files.is_empty()
        }) else {
            return Ok(Some((unreadable_reason(page.free_item.as_ref()), title)));
        };
        let files: Vec<_> = evidence
            .files
            .iter()
            .map(|file| crate::catalogue::terms_only::TermsFile {
                name: file.name.as_deref().unwrap_or(""),
                variant_id: file.variant_id.as_deref(),
            })
            .collect();
        if hide_terms && crate::catalogue::terms_only::terms_only(&files, &draft["publicVariants"])
        {
            return Ok(Some(("FREE_TERMS_ONLY", title)));
        }
        use crate::catalogue::acquisition::{added_time, booth_clock};
        // On BOOTH's clock, like the receipt dates it sorts among.
        let (shown, at) = added_time(import.next_time(), &booth_clock());
        let row = parser::Row {
            source_record_id: free_history::free_history_record_id(product),
            product_id: Some(product.to_owned()),
            name: title,
            acquired_date_text: Some(shown),
            acquired_at: Some(at),
            files: evidence.files.clone(),
            manifest_complete: true,
            ..Default::default()
        };
        import.enriched.insert(product.to_owned(), draft);
        // The import's one checkpoint carries every item it publishes, so an item published
        // earlier stays published.
        let key = selected_checkpoint_key(job, account, "free")?;
        let mut checkpoint = match import.publications.checkpoints.remove(&key) {
            Some(checkpoint) => checkpoint,
            None => Checkpoint::fresh(account, "free", "selected")?,
        };
        checkpoint.details_completed += 1;
        let routes = BTreeMap::from([(row.source_record_id.clone(), route)]);
        self.want_download(product)?;
        self.stage_selected(
            app,
            job,
            account,
            "free",
            checkpoint,
            vec![row],
            routes,
            public,
            &mut import.enriched,
            &mut import.publications,
            &mut BTreeSet::new(),
        )
        .await?;
        self.publish_ready_product(app, job, &mut import.publications, product)?;
        Ok(None)
    }

    /// Asks for the item's files like Download does, so they download once it publishes.
    fn want_download(&self, product: &str) -> Result<()> {
        self.inner.catalogue.put(
            "download_intent",
            product,
            json!({"id":product,"wantDownload":true}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_import_keeps_the_listed_order_without_repeats() {
        let ids = free_import_ids(&json!({"productIds":["900002","900001","900002"]})).unwrap();
        assert_eq!(ids, ["900002", "900001"]);
        for args in [
            json!({"productIds":[]}),
            json!({"productIds":["https://booth.pm/ja/items/900001"]}),
            json!({"productIds":[900001]}),
            json!({}),
        ] {
            assert!(free_import_ids(&args).is_err(), "{args}");
        }
        let most: Vec<String> = (0..FREE_IMPORT_LIMIT)
            .map(|n| (900_000 + n).to_string())
            .collect();
        assert_eq!(
            free_import_ids(&json!({"productIds":most})).unwrap().len(),
            FREE_IMPORT_LIMIT
        );
        let too_many: Vec<String> = (0..=FREE_IMPORT_LIMIT)
            .map(|n| (900_000 + n).to_string())
            .collect();
        assert!(free_import_ids(&json!({"productIds":too_many})).is_err());
    }

    #[test]
    fn items_added_together_never_share_a_time() {
        let mut import = FreeImport::default();
        let times: Vec<_> = (0..50).map(|_| import.next_time()).collect();
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
