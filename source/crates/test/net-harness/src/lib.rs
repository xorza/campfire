//! The network harness: a match played in process between a server and its clients, on the
//! stack's stepper (02-engine-core.md, The harness splits at the network).

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod in_process_match;

pub use crate::in_process_match::link_layout::LinkLayout;
pub use crate::in_process_match::server_record::ServerRecord;
pub use crate::in_process_match::{End, InProcessMatch, InProcessSetup, InProcessSetupBuilder};

pub mod bench;
