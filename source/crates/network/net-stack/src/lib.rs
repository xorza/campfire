//! Plugin groups that install the stack's layer crates in the order they need (11-net-stack/api.md,
//! One plugin group for each kind of app, and no plugin adds another).
//!
//! [`ClientPlugins`] and [`ServerPlugins`], one for each kind of app, are the entry points.
//! Every type comes from its layer crate, as `campfire_net_link::Link` or
//! `campfire_net_replication::Replicate`; this crate re-exports none of them.

mod client_plugins;
#[cfg(feature = "internals")]
mod fixtures;
mod layers;
#[cfg(test)]
mod scenario;
mod server_plugins;

pub use crate::client_plugins::ClientPlugins;
pub use crate::server_plugins::ServerPlugins;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::full_stack::FullStack;
}
