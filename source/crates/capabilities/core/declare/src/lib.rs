//! How the engine's modules declare what they add to a match, and what a module's install reads:
//! the declared capabilities, the declarations each module makes once, and the books.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod declare;

pub use crate::declare::book::Book;
pub use crate::declare::by_type::ByType;
pub use crate::declare::cache::Cache;
pub use crate::declare::callbacks::Callbacks;
pub use crate::declare::capability_set::CapabilitySet;
pub use crate::declare::declarations::{ColumnFill, Declarations};
pub use crate::declare::entity_kind::{EntityKind, KindAudience};
pub use crate::declare::entity_parts::EntityParts;
pub use crate::declare::error::CapabilityError;
pub use crate::declare::install_cx::InstallCx;
pub use crate::declare::kind_spawn::KindSpawn;
pub use crate::declare::kind_visitor::KindVisitor;
pub use crate::declare::module::{Module, ModuleVisitor};
pub use crate::declare::module_set::ModuleSet;
pub use crate::declare::per_type::PerType;
pub use crate::declare::phase::Phase;
pub use crate::declare::report::Report;
pub use crate::declare::scratch::Scratch;
pub use crate::declare::set_place::SetPlace;
pub use crate::declare::spawn_parts::SpawnParts;
