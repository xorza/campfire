use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_core::TickRate;

/// The plugins and the per-link components of one layer's stack, which a
/// `Stepper` runs on each side.
///
/// Each layer's `internals` gives its stack on the one below it, with its test protocol.
pub trait Stack: Copy + Send + Sync + 'static {
    /// Adds the plugins of a client to `app`.
    fn add_client(&self, app: &mut App, tick_rate: TickRate);

    /// Adds the plugins of a server to `app`.
    fn add_server(&self, app: &mut App, tick_rate: TickRate);

    /// Adds what both sides register once their plugins are in, such as a protocol: after
    /// `add_client` on a client's app, and after `add_server` and `add_client` on a host server's.
    fn add_protocol(&self, app: &mut App);

    /// Inserts the components a remote client's link carries.
    fn client_link(&self, link: &mut EntityWorldMut<'_>);

    /// Inserts the components a server's link to a remote client carries.
    fn server_link(&self, link: &mut EntityWorldMut<'_>);

    /// Whether a connected client's app is ready, which ends `Stepper::init`: `world` is the
    /// client's app, or the server's for a host client.
    fn is_ready(&self, world: &World) -> bool;
}
