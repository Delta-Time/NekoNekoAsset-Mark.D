//! Typed requests behind the `*_action` commands. The UI still sends `{action, args}`;
//! each service declares one tagged enum of its actions, and `pnpm bindings` turns it
//! into a TypeScript union with ts-rs.
use anyhow::Result;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// Joins an action and its arguments into one request tagged by `action`.
/// `unknown` is the error code for an action the service lacks, `invalid` for
/// arguments of the wrong shape.
pub(crate) fn parse<T: DeserializeOwned>(
    action: &str,
    args: Value,
    unknown: &'static str,
    invalid: &'static str,
) -> Result<T> {
    let mut request = match args {
        Value::Object(fields) => fields,
        Value::Null => Map::new(),
        _ => anyhow::bail!(invalid),
    };
    request.insert("action".into(), Value::String(action.into()));
    serde_json::from_value(Value::Object(request)).map_err(|error| {
        let code = if error.to_string().starts_with("unknown variant") {
            unknown
        } else {
            invalid
        };
        anyhow::Error::new(error).context(code)
    })
}

/// A request's arguments without its `action`. A request whose argument structs keep every
/// field they were sent gives them back exactly.
pub(crate) fn args_of<T: Serialize>(request: &T) -> Value {
    let mut value = serde_json::to_value(request).unwrap_or_default();
    if let Some(fields) = value.as_object_mut() {
        fields.remove("action");
    }
    value
}

/// Test builds check that a request gives back the arguments it was sent, byte for byte,
/// since services keep some of them in their jobs.
#[cfg(test)]
pub(crate) fn assert_round_trip(kind: &str, sent: &Value, back: &Value) {
    let empty = Value::Object(Map::new());
    let sent = if sent.is_null() { &empty } else { sent };
    if let Some(path) = difference(sent, back, String::new()) {
        panic!("{kind} arguments changed through their type at {path}");
    }
}

/// What changed, for the UI and the services that follow it to reload: the payload of
/// every `nna:changed` event.
#[derive(Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "NativeChange")]
pub(crate) struct Change {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_ids: Option<Vec<String>>,
    /// A job's final state could not be written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_code: Option<String>,
    /// A store sign-in changed, so automatic downloads look again.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_changed: Option<bool>,
}

impl Change {
    pub(crate) fn scope(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            ..Self::default()
        }
    }
    pub(crate) fn job(scope: impl Into<String>, job_id: impl Into<String>) -> Self {
        Self {
            job_id: Some(job_id.into()),
            ..Self::scope(scope)
        }
    }
    pub(crate) fn products(
        scope: impl Into<String>,
        ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            product_ids: Some(ids.into_iter().map(Into::into).collect()),
            ..Self::scope(scope)
        }
    }
}

/// Keeps an explicit `null` apart from a missing field: `Some(Value::Null)` versus
/// `None`. Use with `#[serde(default, deserialize_with = "crate::ipc::present")]`.
pub(crate) fn present<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// A field that may be missing, `null` or a value: `None`, `Some(None)` or
/// `Some(Some(value))`. Use with `#[serde(default, deserialize_with =
/// "crate::ipc::nullable", skip_serializing_if = "Option::is_none")]`.
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Test builds check that what the catalogue writes still reads back as its declared
/// view type, byte for byte, so the generated TypeScript cannot drift from it.
/// `closed` views list every field; the others may carry a source record's own fields.
#[cfg(test)]
pub(crate) fn assert_view<T: DeserializeOwned + serde::Serialize>(
    kind: &str,
    value: &Value,
    closed: bool,
    unknown: impl Fn(&T) -> Vec<String>,
) {
    let view: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("{kind} no longer reads as its view type: {error}"));
    let extra = unknown(&view);
    assert!(
        !closed || extra.is_empty(),
        "{kind} has fields its view type lacks: {extra:?}"
    );
    let back = serde_json::to_value(&view).unwrap();
    if let Some(path) = difference(value, &back, String::new()) {
        panic!("{kind} changed through its view type at {path}");
    }
}

/// The first path where two values differ, with both sides.
#[cfg(test)]
fn difference(left: &Value, right: &Value, path: String) -> Option<String> {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => a.keys().chain(b.keys()).find_map(|key| {
            let path = format!("{path}.{key}");
            match (a.get(key), b.get(key)) {
                (Some(x), Some(y)) => difference(x, y, path),
                (x, y) => Some(format!("{path}: {x:?} became {y:?}")),
            }
        }),
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(index, (x, y))| difference(x, y, format!("{path}[{index}]"))),
        _ => (left != right).then(|| format!("{path}: {left} became {right}")),
    }
}
