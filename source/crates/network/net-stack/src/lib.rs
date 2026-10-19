//! Plugin groups that install the stack's layer crates in the order they need (13-net-stack/api.md,
//! One plugin group for each kind of app, and no plugin adds another).
//!
//! [`ClientPlugins`], [`ServerPlugins`] and [`HostPlugins`], one for each kind of app, are the
//! entry points.
//! Every type comes from its layer crate, as `campfire_net_link::Link` or
//! `campfire_net_replication::Replicate`; this crate re-exports none of them.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod client_plugins;
#[cfg(feature = "internals")]
mod fixtures;
mod host_plugins;
mod layers;
mod protocol_check_plugin;
#[cfg(test)]
mod scenario;
mod server_plugins;

pub use crate::client_plugins::ClientPlugins;
pub use crate::host_plugins::HostPlugins;
pub use crate::server_plugins::ServerPlugins;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::full_stack::FullStack;
}
