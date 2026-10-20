//! A session's visibility: who receives each kind of state, what its spectators see, what a player
//! out of play sees, and how many spectators it admits (10-replication-audiences.md), as a mode's
//! data and a host's settings state it, and the bytes a session's terms carry.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod data_kind;
mod error;
mod mode_visibility;
mod out_view;
mod recipients;
mod setting;
mod spectator_fog;
mod visibility;

pub use crate::data_kind::DataKind;
pub use crate::error::VisibilityDecodeError;
pub use crate::mode_visibility::{HostVisibility, ModeVisibility};
pub use crate::out_view::OutView;
pub use crate::recipients::Recipients;
pub use crate::setting::error::VisibilityError;
pub use crate::setting::{Setting, SettingValue};
pub use crate::spectator_fog::SpectatorFog;
pub use crate::visibility::Visibility;

#[cfg(test)]
mod tests;
