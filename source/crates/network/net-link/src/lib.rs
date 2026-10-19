//! Transport-agnostic link state and link lifecycle markers.
//!
//! A [`Link`] is the stack's transport-neutral boundary between the frame's networking systems and
//! the session under them. The transport and the session hand each other payloads through the
//! link's `SessionQueues`; an IO under the session moves datagrams across a network or an
//! in-process one.
//!
//! [`Linking`], [`Linked`], and [`Unlinked`] are mutually exclusive marker components: the state
//! of the link, kept in step with the entity lifecycle. Server-side fan-out relationships are
//! [`Server`] and [`LinkOf`].

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod link;
mod link_plugin;
mod link_start;
mod link_state;
mod link_systems;
mod local_addr;
mod peer_addr;
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
pub use crate::local_addr::LocalAddr;
pub use crate::peer_addr::PeerAddr;
pub use crate::server::Server;
pub use crate::unlink::Unlink;

pub use crate::server::LinkOf;
pub use crate::server_link_plugin::ServerLinkPlugin;
