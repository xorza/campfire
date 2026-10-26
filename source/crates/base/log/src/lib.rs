//! The engine's log events: the `LogEvent`s a tool reads back from a binary's JSON lines, which
//! `LogLine` reads, and the layer that writes those lines. The libraries emit `tracing` events;
//! `logging` starts a binary's output.

mod json_layer;
mod json_text;
mod log_event;
mod log_line;

pub use crate::json_layer::{check_layer, json_lines};
pub use crate::json_text::JsonText;
pub use crate::log_event::LogEvent;
pub use crate::log_line::{LogLevel, LogLine};

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::json_layer::internals::{
        LogCheck, Scoped, capture, capture_layer, capture_named, round_trip,
    };
}
