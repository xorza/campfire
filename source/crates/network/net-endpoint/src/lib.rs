//! The stack's endpoints as ECS components: each IO's endpoints on their entities, the frame's two
//! steps that hand each link's queues to its session and back, and the link entity of each session
//! (13-net-stack/session.md, One generic `EndpointPlugin<Io>` registers each IO's endpoints).

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod endpoint;
mod endpoint_plugin;
mod link_binding;
mod session_link;

pub use crate::endpoint::Endpoint;
pub use crate::endpoint_plugin::EndpointPlugin;
pub use crate::link_binding::LinkBinding;
