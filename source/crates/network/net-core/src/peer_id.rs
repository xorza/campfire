#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use core::fmt::{Display, Formatter};
use core::net::SocketAddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub enum PeerId {
    Entity(u64),
    /// A peer that a link names by its address.
    #[cfg_attr(feature = "reflect", reflect(ignore))]
    Address(SocketAddr),
    /// Refers to the server
    Server,
}

impl Display for PeerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}
