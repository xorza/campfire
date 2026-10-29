//! The match's names as the load resolves them and the match reads them: a table of names for
//! each kind of id, the tags, the teams and their players, each unit type's params, and the
//! filters of tags and relations. No Rhai type holds them, so a crate that resolves a name needs
//! no script engine; the script view makes its strings of them.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod error;
mod filter;
mod filters;
mod match_names;
mod name_tables;
mod tag_names;
mod teams;
mod typed_list;

pub use crate::error::UnknownName;
pub use crate::filter::Filter;
pub use crate::filters::Filters;
pub use crate::match_names::MatchNames;
pub use crate::name_tables::{KindTexts, NameTables};
pub use crate::tag_names::TagNames;
pub use crate::teams::Teams;
pub use crate::typed_list::{TypedItem, TypedList};
