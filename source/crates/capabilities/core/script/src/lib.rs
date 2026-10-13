//! Rhai host and the core script API.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod script_host;

pub use crate::script_host::budget::Budget;
pub use crate::script_host::error::{NumError, Raised, ScriptError};
pub use crate::script_host::{ScriptHost, ScriptId};

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::script_host::error::internals::Undefined;
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::script_host;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        script_host::bench::script(c);
    }
}
