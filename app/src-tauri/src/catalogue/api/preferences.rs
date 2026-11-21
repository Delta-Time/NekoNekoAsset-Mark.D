//! The App's preferences record, which the UI reads and patches.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;
use ts_rs::TS;

/// The preferences record. The catalogue keeps it as stored; this type describes it and
/// test builds check every write against it. Any key may be missing, and a patch sends
/// only the keys it changes. Numbers keep the form they were written in.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// A built-in theme, or a saved preset's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presets: Option<Vec<ThemePreset>>,
    /// Card thumbnail size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<Number>,
    /// List row height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nav_width: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspector_width: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_columns: Option<Vec<String>>,
    /// Percent, 80 to 300.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rounded_corners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_motion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundled_fonts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_handed_drag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"blur\" | \"hide\" | \"show\" | \"only\"")]
    pub sensitive_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_unavailable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_unclassified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_filter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_single_package: Option<bool>,
    /// Which batch actions the selection bar offers; a patch changes only the keys it sends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_actions: Option<BTreeMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_download_all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_concurrency: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_concurrency: Option<Number>,
    /// Zero for no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_rate_bytes_per_second: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_root_id: Option<String>,
    /// Thumbnail classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_features_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_scopes: Option<crate::ai::jobs::AiScopes>,
    /// By avatar id. Older libraries may hold other shapes, which the UI ignores.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(as = "Option<BTreeMap<String, AvatarSetting>>")]
    pub avatar_settings: Option<BTreeMap<String, Value>>,
    /// Automatic compatible-avatar defaults by avatar id, set after store syncs settle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_automatic_enabled: Option<BTreeMap<String, bool>>,
    /// Avatar names resolved to avatar ids.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_aliases: Option<BTreeMap<String, String>>,
    // Older names that portable backups still carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub language: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub theme_tokens: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub theme_id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub download_rate_limit: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub rate_limit: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub avatar_source_aliases: Option<Value>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A saved theme: its name and CSS color tokens.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThemePreset {
    pub id: String,
    pub name: String,
    pub tokens: BTreeMap<String, String>,
}

/// One avatar's display names, aliases and on/off choice.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct AvatarSetting {
    pub enabled: Option<bool>,
    pub original_name: Option<String>,
    pub english_name: Option<String>,
    pub display_name: Option<String>,
    pub shop_name: Option<String>,
    pub aliases: Option<Vec<String>>,
}

/// The saved preferences, with `restartRequired` when they finished setup.
#[cfg(test)]
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(super) struct PreferencesSaved {
    #[serde(flatten)]
    preferences: Preferences,
    #[ts(optional)]
    restart_required: Option<bool>,
}
