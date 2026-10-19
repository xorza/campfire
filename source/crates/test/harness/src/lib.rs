//! The match harness of the session's and the network's tests: fixed and scripted sessions, their
//! goldens and records, the checks of copies and restores, and a match played in process between
//! a server and its clients over modeled links.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod archetype_watch;
mod arena;
mod copy_check;
mod fixed_session;
mod golden;
mod hash_trail;
mod in_process_match;
mod match_record;
mod match_units;
mod moba_3v3;
mod proving_match;
mod scripted;
mod scripted_mode;

pub use crate::archetype_watch::ArchetypeWatch;
pub use crate::arena::Arena;
pub use crate::bench::allocations::{Allocations, CountingAllocator};
pub use crate::fixed_session::FixedSession;
pub use crate::fixed_session::fixed_match::FixedMatch;
pub use crate::hash_trail::{Difference, HashTrail};
pub use crate::in_process_match::link_model::LinkModel;
pub use crate::in_process_match::{End, InProcessMatch, InProcessSetup, LinkLayout};
pub use crate::match_record::MatchRecord;
pub use crate::match_units::MatchUnits;
pub use crate::moba_3v3::Moba3v3;
pub use crate::proving_match::ProvingMatch;
pub use crate::scripted::{Aim, Plan};
pub use crate::scripted_mode::ScriptedMode;

pub mod bench;
