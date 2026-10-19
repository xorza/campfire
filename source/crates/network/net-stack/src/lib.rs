//! Plugin groups that install the stack's layer crates in the order they need.
//!
//! [`client_plugins::ClientPlugins`] and [`server_plugins::ServerPlugins`] are the entry points.
//! Every type comes from its layer crate, as `campfire_net_link::Link` or
//! `campfire_net_replication::Replicate`; this crate re-exports none of them.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod client_plugins;
#[cfg(feature = "internals")]
pub mod internals;
mod protocol_check_plugin;
#[cfg(test)]
mod scenario;
mod server_plugins;
mod shared_plugin;

pub use crate::client_plugins::ClientPlugins;
pub use crate::server_plugins::ServerPlugins;
