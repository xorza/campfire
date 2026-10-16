//! A session's visibility: who receives each kind of state, what its spectators see, what a player
//! out of play sees, and how many spectators it admits (14-replication-audiences.md), as a mode's
//! data and a host's settings state it, and the bytes a session's terms carry.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod mode_visibility;
mod out_view;
mod setting;
mod spectator_fog;
mod visibility;

pub use crate::error::VisibilityDecodeError;
pub use crate::mode_visibility::{HostVisibility, ModeVisibility};
pub use crate::out_view::OutView;
pub use crate::setting::error::VisibilityError;
pub use crate::setting::{Setting, SettingValue};
pub use crate::spectator_fog::SpectatorFog;
pub use crate::visibility::Visibility;

#[cfg(test)]
mod tests;
