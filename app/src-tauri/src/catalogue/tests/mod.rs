use super::*;

mod compatible_avatars;
mod edits;
mod files;
mod jobs_and_accounts;
mod listing;
mod persistence;
mod purchases;

fn dynamic_details(c: &Catalogue, id: &str) -> Value {
    c.command("product_details", json!({"id":id})).unwrap()
}

fn dynamic_tag_ids(c: &Catalogue, id: &str) -> BTreeSet<String> {
    dynamic_details(c, id)["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| matches!(text(e, "source"), "sourceTag" | "userTag"))
        .map(|e| text(e, "avatarId").to_owned())
        .collect()
}

fn synthetic_named_base(c: &Catalogue, id: &str, name: &str) {
    c.put(
        "product",
        id,
        json!({"id":id,"sourceTitle":name,"sourceCategory":{"name":"3Dアバター"}}),
    )
    .unwrap();
}

fn seed_current_product_url(c: &Catalogue, id: &str) -> Value {
    let value = json!({"id":id,"store":"BOOTH","title":"Synthetic current source","sourceUrl":format!("https://booth.pm/ja/items/{id}"),"observedAt":"2026-10-09T00:00:00Z","sourceDescription":format!("Preserve literal https://booth.pm/items/{id} in prose")});
    c.mutate(|tx, projections| {
        let before = raw(tx, "product", id)?;
        Catalogue::enqueue_sync(tx, "product", id, before.as_ref(), Some(&value))?;
        set_raw(tx, "product", id, &value)?;
        projections.merge(projection::refresh(tx, id)?);
        Ok(())
    })
    .unwrap();
    value
}

/// Clothing on an owned variant, optionally with a public support section.
fn owned_variant_product(c: &Catalogue, id: &str, variant: &str, support: Value) {
    c.put_many(vec![
        ("product".into(), id.into(), json!({"id":id,"store":"BOOTH","sourceTitle":"Authored VRChat clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"],
            "publicVariants":[{"id":format!("{id}-v"),"type":"digital","name":variant}],"publicSupportAvatars":support})),
        ("remote_file".into(), format!("{id}-f"), json!({"id":format!("{id}-f"),"productId":id,"accountId":"a","variantId":format!("{id}-v"),"name":"opaque.zip","revision":"r1"})),
        ("entitlement".into(), format!("{id}-e"), json!({"id":format!("{id}-e"),"productId":id,"accountId":"a","fileIds":[format!("{id}-f")],"manifestComplete":true})),
    ]).unwrap();
}

/// Clothing whose support section links `item`, on an owned full pack.
fn citing_product(c: &Catalogue, id: &str, item: &str, label: &str) {
    owned_variant_product(
        c,
        id,
        "全部入りフルパック",
        json!([{"id":format!("booth:{item}"),"productId":item,"officialName":label,"displayName":label}]),
    );
}

fn avatar_rows(c: &Catalogue, kind: &str) -> Vec<Value> {
    c.command("avatar_options", json!({})).unwrap()["avatarOptions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|option| text(option, "id").starts_with(kind))
        .cloned()
        .collect()
}

fn avatar_filter(c: &Catalogue, avatar: &Value) -> Vec<String> {
    let mut ids = strings(&c.command("query", json!({"avatar":avatar})).unwrap()["matchedIds"]);
    ids.sort();
    ids
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/catalogue-synthetic")
            .join(format!("nna-catalogue-synthetic-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn open(&self) -> Catalogue {
        Catalogue::open(&self.0).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn product(c: &Catalogue, id: &str) {
    c.put("product",id,json!({"id":id,"title":"Synthetic ＡＢＣ 髪型","shop":"Synthetic Maker","tags":["hair","blue"],"sourceCategory":{"name":"Hair"}})).unwrap();
}

fn projection(c: &Catalogue, id: &str) -> Value {
    c.command("bootstrap", json!({})).unwrap()["products"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| text(p, "id") == id)
        .unwrap()
        .clone()
}
