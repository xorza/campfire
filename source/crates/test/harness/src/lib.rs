//! The session harness: fixed and scripted sessions, their goldens and records, and the checks of
//! copies and restores, with no network (02-engine-core.md, The harness splits at the network).

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod archetype_watch;
mod copy_check;
mod error;
mod fixed_session;
mod golden;
mod hash_trail;
mod hero_walk;
mod match_record;
mod match_units;
mod moba_3v3;
mod probe_file;
mod proving_match;
mod replay_record;
mod scripted;
mod scripted_mode;

pub use crate::archetype_watch::ArchetypeWatch;
pub use crate::error::ProbeWriteError;
pub use crate::fixed_session::FixedSession;
pub use crate::fixed_session::fixed_match::{CheckedMatch, FixedMatch};
pub use crate::hash_trail::{Difference, HashTrail};
pub use crate::hero_walk::HeroWalk;
pub use crate::match_record::MatchRecord;
pub use crate::match_units::MatchUnits;
pub use crate::moba_3v3::Moba3v3;
pub use crate::probe_file::ProbeFile;
pub use crate::proving_match::ProvingMatch;
pub use crate::replay_record::ReplayRecord;
pub use crate::scripted::{CastAim, Plan};
pub use crate::scripted_mode::ScriptedMode;

pub mod bench;
