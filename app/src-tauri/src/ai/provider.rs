use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiConfig {
    #[serde(default)]
    pub entry_id: Option<String>,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiWire {
    Openai,
    Anthropic,
    Gemini,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub endpoint: &'static str,
    pub model: &'static str,
    pub wire: AiWire,
    pub vision_supported: bool,
    pub key_required: bool,
}

pub fn presets() -> Vec<AiProviderPreset> {
    [
        (
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            "gpt-4o-mini",
            AiWire::Openai,
            true,
            true,
        ),
        (
            "deepseek",
            "DeepSeek",
            "https://api.deepseek.com",
            "deepseek-flash",
            AiWire::Openai,
            true,
            true,
        ),
        (
            "qwen",
            "Qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "qwen-plus",
            AiWire::Openai,
            false,
            true,
        ),
        (
            "glm",
            "GLM",
            "https://open.bigmodel.cn/api/paas/v4",
            "glm-5.3",
            AiWire::Openai,
            false,
            true,
        ),
        (
            "kimi",
            "Kimi",
            "https://api.moonshot.ai/v1",
            "kimi-k3",
            AiWire::Openai,
            true,
            true,
        ),
        (
            "anthropic",
            "Anthropic",
            "https://api.anthropic.com/v1",
            "claude-sonnet-5-5",
            AiWire::Anthropic,
            true,
            true,
        ),
        (
            "gemini",
            "Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
            "gemini-3.8-flash",
            AiWire::Gemini,
            true,
            true,
        ),
        (
            "openrouter",
            "OpenRouter",
            "https://openrouter.ai/api/v1",
            "openai/gpt-4o-mini",
            AiWire::Openai,
            true,
            true,
        ),
        (
            "local",
            "Local / OpenAI-compatible",
            "http://localhost:1234/v1",
            "local-model",
            AiWire::Openai,
            true,
            false,
        ),
        (
            "custom",
            "OpenAI-compatible",
            "https://api.openai.com/v1",
            "gpt-4o-mini",
            AiWire::Openai,
            true,
            true,
        ),
    ]
    .into_iter()
    .map(
        |(id, label, endpoint, model, wire, vision_supported, key_required)| AiProviderPreset {
            id,
            label,
            endpoint,
            model,
            wire,
            vision_supported,
            key_required,
        },
    )
    .collect()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            entry_id: None,
            provider: "openai".into(),
            endpoint: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
        }
    }
}

impl AiConfig {
    pub fn validated(mut self) -> Result<Self> {
        ensure!(
            presets().iter().any(|p| p.id == self.provider),
            "ai_config_invalid"
        );
        let url =
            url::Url::parse(self.endpoint.trim()).map_err(|_| anyhow!("ai_config_invalid"))?;
        let loopback = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.host_str().is_some()
                && (url.scheme() == "https"
                    || (url.scheme() == "http" && self.provider == "local" && loopback)),
            "ai_config_invalid"
        );
        ensure!(
            !self.endpoint.contains('\\')
                && !self.endpoint.contains('%')
                && self.endpoint.len() <= 2048,
            "ai_config_invalid"
        );
        if self.provider == "local" {
            ensure!(loopback, "ai_config_invalid");
        }
        self.endpoint = url.as_str().trim_end_matches('/').to_owned();
        self.model = self.model.trim().to_owned();
        ensure!(
            !self.model.is_empty()
                && self.model.len() <= 128
                && self
                    .model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
                && !self
                    .model
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == ".."),
            "ai_config_invalid"
        );
        Ok(self)
    }
    pub fn wire(&self) -> AiWire {
        match self.provider.as_str() {
            "anthropic" => AiWire::Anthropic,
            "gemini" => AiWire::Gemini,
            _ => AiWire::Openai,
        }
    }
    pub fn key_required(&self) -> bool {
        self.provider != "local"
    }
    pub fn vision_supported(&self) -> bool {
        match self.provider.as_str() {
            "qwen" => self.model.contains("vl") || self.model.contains("vision"),
            "glm" => self.model.contains('v') || self.model.contains("vision"),
            _ => true,
        }
    }
    pub fn request_url(&self) -> String {
        match self.wire() {
            AiWire::Openai => format!("{}/chat/completions", self.endpoint),
            AiWire::Anthropic => format!("{}/messages", self.endpoint),
            AiWire::Gemini => format!("{}/models/{}:generateContent", self.endpoint, self.model),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Settings {
    version: u32,
    active_entry_id: Option<String>,
    entries: BTreeMap<String, AiConfig>,
}

fn settings(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings {
            version: 1,
            active_entry_id: None,
            entries: BTreeMap::new(),
        });
    }
    ensure!(
        fs::metadata(path)
            .map_err(|_| anyhow!("ai_config_invalid"))?
            .len()
            <= 256 * 1024,
        "ai_config_invalid"
    );
    let mut settings: Settings =
        serde_json::from_slice(&fs::read(path).map_err(|_| anyhow!("ai_config_invalid"))?)
            .map_err(|_| anyhow!("ai_config_invalid"))?;
    ensure!(
        settings.version == 1 && settings.entries.len() <= 64,
        "ai_config_invalid"
    );
    for (id, config) in &mut settings.entries {
        ensure!(
            uuid::Uuid::parse_str(id).is_ok() && config.entry_id.as_ref() == Some(id),
            "ai_config_invalid"
        );
        *config = config.clone().validated()?;
    }
    ensure!(
        settings
            .active_entry_id
            .as_ref()
            .is_none_or(|id| settings.entries.contains_key(id))
            && (settings.entries.is_empty() == settings.active_entry_id.is_none()),
        "ai_config_invalid"
    );
    Ok(settings)
}

pub fn list_entries(path: &Path) -> Result<(Vec<AiConfig>, Option<String>)> {
    let settings = settings(path)?;
    Ok((
        settings.entries.into_values().collect(),
        settings.active_entry_id,
    ))
}

pub fn load_config(path: &Path) -> Result<AiConfig> {
    let settings = settings(path)?;
    settings
        .active_entry_id
        .and_then(|id| settings.entries.get(&id).cloned())
        .ok_or_else(|| anyhow!("ai_not_configured"))
}

pub fn entry_config(path: &Path, id: &str) -> Result<AiConfig> {
    settings(path)?
        .entries
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow!("ai_entry_missing"))
}

pub fn select_entry(path: &Path, id: &str) -> Result<()> {
    let mut settings = settings(path)?;
    ensure!(settings.entries.contains_key(id), "ai_entry_missing");
    settings.active_entry_id = Some(id.to_owned());
    write_settings(path, settings)
}

pub fn remove_entry(path: &Path, id: &str) -> Result<()> {
    let mut settings = settings(path)?;
    ensure!(settings.entries.remove(id).is_some(), "ai_entry_missing");
    if settings.active_entry_id.as_deref() == Some(id) {
        settings.active_entry_id = settings.entries.keys().next().cloned();
    }
    write_settings(path, settings)
}

pub fn save_entry(path: &Path, config: AiConfig) -> Result<()> {
    let config = config.validated()?;
    let id = config
        .entry_id
        .clone()
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| anyhow!("ai_config_invalid"))?;
    let mut settings = settings(path)?;
    ensure!(settings.entries.len() < 64, "ai_entry_limit");
    ensure!(!settings.entries.contains_key(&id), "ai_config_invalid");
    settings.active_entry_id = Some(id.clone());
    settings.entries.insert(id, config);
    write_settings(path, settings)
}

fn write_settings(path: &Path, settings: Settings) -> Result<()> {
    let bytes = serde_json::to_vec(&settings).map_err(|_| anyhow!("ai_config_invalid"))?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .context("ai_settings_write_failed")?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.context("ai_settings_write_failed")
}
#[cfg(windows)]
pub(crate) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 1 | 8) } == 0 {
        return Err(
            anyhow::Error::new(std::io::Error::last_os_error()).context("file_replace_failed")
        );
    }
    Ok(())
}
#[cfg(not(windows))]
pub(crate) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target)?;
    Ok(())
}

// Translate a bounded internal chat request. No caller-controlled tools or URLs are executed.
pub fn request_payload(config: &AiConfig, mut payload: Value) -> Result<Value> {
    payload["model"] = json!(config.model);
    let schema = payload.pointer("/response_format/json_schema").cloned();
    match config.wire() {
        AiWire::Openai => {
            if config.provider != "openai" {
                if let Some(tokens) = payload
                    .as_object_mut()
                    .and_then(|p| p.remove("max_completion_tokens"))
                {
                    payload["max_tokens"] = tokens;
                }
                payload.as_object_mut().unwrap().remove("store");
            }
            if ["deepseek", "qwen", "glm", "kimi"].contains(&config.provider.as_str()) {
                if let Some(schema) = schema {
                    payload["response_format"] = json!({"type":"json_object"});
                    if config.provider == "glm" && config.vision_supported() {
                        payload.as_object_mut().unwrap().remove("response_format");
                    }
                    let messages = payload["messages"]
                        .as_array_mut()
                        .ok_or_else(|| anyhow!("ai_malformed"))?;
                    messages.insert(0,json!({"role":"system","content":format!("Return only valid JSON conforming to this schema: {}",schema["schema"])}));
                }
            }
            if config.provider == "deepseek" {
                payload["thinking"] = json!({"type":"disabled"});
            }
            Ok(payload)
        }
        AiWire::Anthropic | AiWire::Gemini => {
            let messages = payload["messages"]
                .as_array()
                .ok_or_else(|| anyhow!("ai_malformed"))?;
            let mut system = Vec::new();
            let mut turns = Vec::new();
            for message in messages {
                let role = message["role"]
                    .as_str()
                    .ok_or_else(|| anyhow!("ai_malformed"))?;
                if role == "system" {
                    system.push(
                        message["content"]
                            .as_str()
                            .ok_or_else(|| anyhow!("ai_malformed"))?
                            .to_owned(),
                    );
                    continue;
                }
                let content = if let Some(text) = message["content"].as_str() {
                    vec![json!({"type":"text","text":text})]
                } else {
                    message["content"]
                        .as_array()
                        .ok_or_else(|| anyhow!("ai_malformed"))?
                        .clone()
                };
                let mut parts = Vec::new();
                for part in content {
                    if part["type"] == "text" {
                        parts.push(if config.wire() == AiWire::Anthropic {
                            json!({"type":"text","text":part["text"]})
                        } else {
                            json!({"text":part["text"]})
                        });
                    } else if part["type"] == "image_url" {
                        let data = part
                            .pointer("/image_url/url")
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow!("ai_image_invalid"))?;
                        let (mime, bytes) = data
                            .strip_prefix("data:")
                            .and_then(|s| s.split_once(";base64,"))
                            .ok_or_else(|| anyhow!("ai_image_invalid"))?;
                        parts.push(if config.wire()==AiWire::Anthropic{json!({"type":"image","source":{"type":"base64","media_type":mime,"data":bytes}})}else{json!({"inlineData":{"mimeType":mime,"data":bytes}})});
                    } else {
                        return Err(anyhow!("ai_malformed"));
                    }
                }
                turns.push(if config.wire() == AiWire::Anthropic {
                    json!({"role":role,"content":parts})
                } else {
                    json!({"role":if role=="assistant"{"model"}else{"user"},"parts":parts})
                });
            }
            let tokens = payload["max_completion_tokens"].clone();
            if config.wire() == AiWire::Anthropic {
                let mut out = json!({"model":config.model,"system":system.join("\n"),"messages":turns,"max_tokens":tokens,"stream":false});
                if let Some(mut schema) = schema {
                    anthropic_schema(&mut schema["schema"]);
                    out["output_config"] =
                        json!({"format":{"type":"json_schema","schema":schema["schema"]}});
                }
                Ok(out)
            } else {
                let mut out = json!({"systemInstruction":{"parts":[{"text":system.join("\n")}]},"contents":turns,"generationConfig":{"maxOutputTokens":tokens}});
                if let Some(schema) = schema {
                    out["generationConfig"]["responseFormat"] =
                        json!({"text":{"mimeType":"APPLICATION_JSON","schema":schema["schema"]}});
                }
                Ok(out)
            }
        }
    }
}

// Anthropic's raw-schema API rejects numeric/string bounds and maxItems.
// Preserve them as instructions while the unchanged native parser enforces them.
fn anthropic_schema(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            let mut constraints = Vec::new();
            for key in [
                "minimum",
                "maximum",
                "exclusiveMinimum",
                "exclusiveMaximum",
                "multipleOf",
                "minLength",
                "maxLength",
                "maxItems",
            ] {
                if let Some(value) = object.remove(key) {
                    constraints.push(format!("{key}: {value}"));
                }
            }
            if object
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|value| value > 1)
            {
                if let Some(value) = object.remove("minItems") {
                    constraints.push(format!("minItems: {value}"));
                }
            }
            if !constraints.is_empty() {
                let description = object
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                object.insert(
                    "description".into(),
                    json!(format!(
                        "{description} Constraints: {}.",
                        constraints.join(", ")
                    )),
                );
            }
            for value in object.values_mut() {
                anthropic_schema(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                anthropic_schema(value);
            }
        }
        _ => {}
    }
}

pub fn response_payload(wire: AiWire, value: Value) -> Result<Value> {
    if wire == AiWire::Openai {
        return Ok(value);
    }
    let (content, finish) = if wire == AiWire::Anthropic {
        let content = value["content"]
            .as_array()
            .ok_or_else(|| anyhow!("ai_malformed"))?
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        let finish = match value["stop_reason"].as_str() {
            Some("end_turn" | "stop_sequence") => "stop",
            Some("max_tokens") => "length",
            Some("refusal") => "content_filter",
            _ => "other",
        };
        (content, finish)
    } else {
        if value.pointer("/promptFeedback/blockReason").is_some() {
            return Err(anyhow!("ai_refused"));
        }
        let candidate = value
            .pointer("/candidates/0")
            .ok_or_else(|| anyhow!("ai_malformed"))?;
        let content = candidate
            .pointer("/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part["thought"] != true)
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        let finish = match candidate["finishReason"].as_str() {
            Some("STOP") => "stop",
            Some("MAX_TOKENS") => "length",
            Some("SAFETY" | "RECITATION" | "PROHIBITED_CONTENT") => "content_filter",
            _ => "other",
        };
        (content, finish)
    };
    Ok(json!({"choices":[{"finish_reason":finish,"message":{"content":content}}]}))
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
