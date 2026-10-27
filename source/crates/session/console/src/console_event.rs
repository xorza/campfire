#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An event of a game's log: its level, the module it came from, the tick it names, when it names
/// one, its message, and its other fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConsoleEvent {
    pub level: EventLevel,
    pub target: String,
    pub tick: Option<u64>,
    pub message: Option<String>,
    pub fields: BTreeMap<String, FieldValue>,
}

/// An event's level, as `tracing` names them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// A field's value, as an event records it: a value it writes as text, as its `Display` or its
/// `Debug` does, is text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FieldValue {
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
}
