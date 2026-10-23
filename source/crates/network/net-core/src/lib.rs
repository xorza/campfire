//! The vocabulary every stack layer shares, with no engine in it: the app's [`Instant`], a link's
//! [`LinkDirection`], ticks and their deltas and instants, peer and network ids, the maps from
//! types to network ids and the registries' hashes, and a link's [`Random`] bytes
//! (11-net-stack/layering.md, The vocabulary is plain, and the clock and the timelines a glue crate
//! of their own).

mod instant;
mod link_direction;
mod net_id;
mod net_tick;
mod overstep;
mod peer_id;
mod random;
mod registry_hasher;
mod registry_seal;
mod tick_buffer;
mod tick_range_error;
mod tick_time;
mod ticked;
mod time_source;
mod tls_key;
mod type_mapper;

pub use crate::instant::Instant;
pub use crate::link_direction::{LinkDirection, LinkDirections};
pub use crate::net_id::NetId;
pub use crate::net_tick::NetTick;
pub use crate::overstep::Overstep;
pub use crate::peer_id::PeerId;
pub use crate::random::Random;
pub use crate::registry_hasher::{RegistryHash, RegistryHashMeaning, RegistryHasher};
pub use crate::registry_seal::RegistrySeal;
pub use crate::tick_buffer::{Drain, IntoIter, Iter, TickBuffer};
pub use crate::tick_range_error::TickRangeError;
pub use crate::tick_time::PositiveTickSpan;
pub use crate::tick_time::TickInstant;
pub use crate::tick_time::TickSpan;
pub use crate::ticked::Ticked;
pub use crate::time_source::TimeSource;
pub use crate::tls_key::{TlsKey, TlsKeyMeaning};
pub use crate::type_mapper::{RegisteredType, TypeMapper};
