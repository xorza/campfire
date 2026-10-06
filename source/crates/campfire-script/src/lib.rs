//! Rhai host and the core script API.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod script_host;

pub use crate::script_host::budget::Budget;
pub use crate::script_host::error::{NumError, Raised, ScriptError};
pub use crate::script_host::{ScriptHost, ScriptId};
/// The script engine the host runs, at the version it pins, for capabilities to register their
/// script API with.
pub use rhai;
