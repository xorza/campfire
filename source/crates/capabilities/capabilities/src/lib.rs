//! Capabilities: the mechanisms a mode combines, a crate each, on the core, and the mode above
//! them. This crate is the top of them: `release` installs every module, `books` builds a
//! match's books, and `Order` is the body of a command, by the capability that owns it. Its
//! `pub use` list names only its own items: a crate above depends on the crate of each type it
//! names.
//!
//! A capability crate depends only on the crates of lower layers, each one layer above its
//! highest dependency, as a test of `release` reads them from the manifests (02-engine-core.md,
//! Layers). A capability gives the script view its fields of a unit through its column, and the
//! call frame what a call reads besides the core's through its part.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod books;
#[cfg(feature = "internals")]
mod fixtures;
mod order;
mod release;

pub use crate::books::Books;
pub use crate::books::book_input::BookInput;
pub use crate::books::load_names::LoadNames;
pub use crate::books::package_content::{ContentSeed, PackageContent};
pub use crate::order::{Directive, Order};
pub use crate::release::Release;
pub use crate::release::error::CapabilityError;
pub use crate::release::r#match::Match;
pub use crate::release::match_kind::{MatchKind, ModeStart, PackageScript};
pub use crate::release::match_setup::MatchSetup;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::abilities::AbilitiesFixtures;
    pub use crate::fixtures::actions::{ActionsFixtures, WorldActionNames};
    pub use crate::fixtures::areas::AreasFixtures;
    pub use crate::fixtures::navigation::NavigationFixtures;
    pub use crate::fixtures::orders::OrdersFixtures;
    pub use crate::fixtures::production::ProductionFixtures;
    pub use crate::fixtures::units::{FilterFixtures, UnitsFixtures, ViewFixtures};
    pub use crate::fixtures::vision::VisionFixtures;
}

#[cfg(feature = "internals")]
pub mod bench;
