use std::fmt::{self, Debug, Display};

use serde::de::value::{
    BoolDeserializer, F64Deserializer, I64Deserializer, MapDeserializer, StringDeserializer,
    U64Deserializer,
};
use serde::de::{self, DeserializeOwned, Deserializer, IntoDeserializer, Visitor};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::log_event::LogEvent;
use crate::log_line::LogLevel;

/// An event as a layer takes it, its fields typed as tracing records them: what a console reads,
/// with no line written and parsed back.
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub level: LogLevel,
    pub target: &'static str,
    /// Each field, in the order the event records them, `message` among them.
    pub fields: Vec<(&'static str, LogValue)>,
}

/// A field's value as tracing records it: one it writes by `Debug` or `Display` is text.
#[derive(Debug, Clone, PartialEq)]
pub enum LogValue {
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
}

/// The layer that hands each event's record to its function, on the thread that logs it, as a
/// console's tap takes them.
pub struct RecordLayer<F>(F);

impl<F> Debug for RecordLayer<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecordLayer")
    }
}

impl<F: Fn(&LogRecord) + Send + Sync + 'static> RecordLayer<F> {
    pub const fn new(take: F) -> Self {
        Self(take)
    }
}

impl<S: Subscriber, F: Fn(&LogRecord) + Send + Sync + 'static> Layer<S> for RecordLayer<F> {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        (self.0)(&LogRecord::of(event));
    }
}

/// Why a record's fields do not read as an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFieldsError(String);

impl Display for LogFieldsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LogFieldsError {}

impl de::Error for LogFieldsError {
    fn custom<T: Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl LogRecord {
    /// The record of `event`.
    pub fn of(event: &Event<'_>) -> Self {
        let metadata = event.metadata();
        let mut record = Self {
            level: LogLevel::of(*metadata.level()),
            target: metadata.target(),
            fields: Vec::with_capacity(metadata.fields().len()),
        };
        event.record(&mut record);
        record
    }

    /// The text of field `field`; none for a field it lacks, or one of no text.
    pub fn text(&self, field: &str) -> Option<&str> {
        self.fields.iter().find_map(|(name, value)| match value {
            LogValue::Text(text) if *name == field => Some(text.as_str()),
            _ => None,
        })
    }

    /// The event of type `E`, when the record has its message; an error when its fields do not
    /// read as `E`.
    pub fn read<E: LogEvent>(&self) -> Option<Result<E, LogFieldsError>> {
        (self.text("message")? == E::MESSAGE).then(|| self.read_fields())
    }

    /// Its fields, read as a `T`, as a JSON object of them reads.
    fn read_fields<T: DeserializeOwned>(&self) -> Result<T, LogFieldsError> {
        let fields = self
            .fields
            .iter()
            .map(|(name, value)| (*name, value.clone()));
        T::deserialize(MapDeserializer::new(fields))
    }
}

impl LogLevel {
    /// The level of `level`, as tracing names it.
    const fn of(level: Level) -> Self {
        match level {
            Level::TRACE => Self::Trace,
            Level::DEBUG => Self::Debug,
            Level::INFO => Self::Info,
            Level::WARN => Self::Warn,
            _ => Self::Error,
        }
    }
}

/// Each field as tracing records it.
impl Visit for LogRecord {
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push((field.name(), LogValue::Bool(value)));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push((field.name(), LogValue::Integer(value)));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push((field.name(), LogValue::Unsigned(value)));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.push((field.name(), LogValue::Float(value)));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name(), LogValue::Text(value.to_owned())));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.fields
            .push((field.name(), LogValue::Text(format!("{value:?}"))));
    }
}

impl IntoDeserializer<'_, LogFieldsError> for LogValue {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

/// A field's value, as a JSON value of it reads: a number as its width asks, text as a string, a
/// unit variant by its name, and a present value as `Some`.
impl<'de> Deserializer<'de> for LogValue {
    type Error = LogFieldsError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, LogFieldsError> {
        match self {
            Self::Bool(value) => BoolDeserializer::new(value).deserialize_any(visitor),
            Self::Integer(value) => I64Deserializer::new(value).deserialize_any(visitor),
            Self::Unsigned(value) => U64Deserializer::new(value).deserialize_any(visitor),
            Self::Float(value) => F64Deserializer::new(value).deserialize_any(visitor),
            Self::Text(value) => StringDeserializer::new(value).deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, LogFieldsError> {
        visitor.visit_some(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, LogFieldsError> {
        match self {
            Self::Text(value) => {
                StringDeserializer::new(value).deserialize_enum(name, variants, visitor)
            }
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, LogFieldsError> {
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit
        unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

#[cfg(test)]
mod tests;
