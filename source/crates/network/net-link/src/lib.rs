//! The link: the entity of one connection, its state, the queues it shares with its packet
//! session, and a server's links (11-net-stack/packet-session.md, The seam is a set of queues on
//! `Link`).

mod link;
mod link_plugin;
mod link_start;
mod link_state;
mod link_systems;
mod local_address;
mod remote_address;
mod server;
mod server_link_plugin;
mod unlink;

pub use crate::link::Link;
pub use crate::link_plugin::LinkPlugin;
pub use crate::link_start::LinkStart;
pub use crate::link_state::Linked;
pub use crate::link_state::Linking;
pub use crate::link_state::Unlinked;
pub use crate::link_systems::LinkSystems;
pub use crate::local_address::LocalAddress;
pub use crate::remote_address::RemoteAddress;
pub use crate::server::Server;
pub use crate::unlink::Unlink;

pub use crate::server::LinkOf;
pub use crate::server_link_plugin::ServerLinkPlugin;
