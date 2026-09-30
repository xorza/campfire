//! Rhai host and the core script API.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod script_host;

pub use error::{NumError, Raised, ScriptError};
/// The script engine the host runs, at the version it pins, for capabilities to register their
/// script API with.
pub use rhai;
pub use script_host::budget::Budget;
pub use script_host::{ScriptHost, ScriptId};
