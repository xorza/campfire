//! The stack's endpoints as ECS components: each IO's endpoints on their entities, the frame's two
//! steps that hand each link's queues to its packet session and back, and the link entity of each
//! packet session (11-net-stack/packet-session.md, One generic `EndpointPlugin<Io>` registers each
//! IO's endpoints).

mod endpoint;
mod endpoint_plugin;
mod frame_wake;
mod link_binding;
mod packet_session_link;

pub use crate::endpoint::Endpoint;
pub use crate::endpoint_plugin::{EndpointPlugin, PacketSessionLinkPlugin};
pub use crate::frame_wake::FrameWake;
pub use crate::link_binding::LinkBinding;
