//! How the engine's modules declare what they add to a match, and what a module's install reads:
//! the declared capabilities, the declarations each module makes once, and the books.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod book;
mod by_type;
mod cache;
mod callback_table;
mod callbacks;
mod capability_set;
mod declarations;
mod entity_kind;
mod entity_parts;
mod error;
mod install_cx;
mod kind_spawn;
mod kind_visitor;
mod module;
mod module_set;
mod one_shot;
mod per_type;
mod phase;
mod report;
mod role;
mod role_form;
mod scratch;
mod server_only;
mod set_place;
mod spawn_parts;

pub use crate::book::Book;
pub use crate::by_type::ByType;
pub use crate::cache::Cache;
pub use crate::callback_table::CallbackTable;
pub use crate::callbacks::Callbacks;
pub use crate::capability_set::CapabilitySet;
pub use crate::declarations::{ColumnFill, Declarations};
pub use crate::entity_kind::{EntityKind, KindAudience};
pub use crate::entity_parts::EntityParts;
pub use crate::error::CapabilityError;
pub use crate::install_cx::{InstallCx, MatchFacts};
pub use crate::kind_spawn::KindSpawn;
pub use crate::kind_visitor::KindVisitor;
pub use crate::module::{Module, ModuleVisitor};
pub use crate::module_set::ModuleSet;
pub use crate::one_shot::OneShot;
pub use crate::per_type::PerType;
pub use crate::phase::Phase;
pub use crate::report::Report;
pub use crate::role::Role;
pub use crate::role_form::{ClientForm, RoleForm, ServerForm};
pub use crate::scratch::Scratch;
pub use crate::server_only::ServerOnly;
pub use crate::set_place::SetPlace;
pub use crate::spawn_parts::SpawnParts;
