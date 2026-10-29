//! How the engine's modules declare what they add to a match, and what a module's install reads:
//! the declared capabilities, the declarations each module makes once, and the books.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod book;
mod book_cx;
mod book_source;
mod built_books;
mod by_id;
mod cache;
mod callback_table;
mod callbacks;
mod capability_set;
mod declarations;
mod entity_kind;
mod entity_parts;
mod error;
mod fact;
mod install_cx;
mod kind_spawn;
mod kind_visitor;
mod match_facts;
mod module;
mod module_systems;
mod name_sets;
mod named_scripts;
mod named_texts;
mod one_shot;
mod per_id;
mod report;
mod role;
mod role_form;
mod scratch;
mod section;
mod server_only_systems;
mod spawn_parts;
mod state_fields;
mod unit_defaults;
mod unit_type_data;

pub use crate::book::Book;
pub use crate::book_cx::{BookCx, BuildFn, ExtendFn, NamesFn, RuleFn};
pub use crate::book_source::{BookSource, Given, ScriptAt, SourceKind, SourcePackage, SourceType};
pub use crate::built_books::BuiltBooks;
pub use crate::by_id::{ById, ByIdBuilder, ByIdIter};
pub use crate::cache::Cache;
pub use crate::callback_table::CallbackTable;
pub use crate::callbacks::Callbacks;
pub use crate::capability_set::CapabilitySet;
pub use crate::declarations::{ColumnFill, Declarations};
pub use crate::entity_kind::{EntityKind, KindAudience};
pub use crate::entity_parts::EntityParts;
pub use crate::error::{
    BookError, BookPlace, DefaultsError, NamesListError, PatchError, Problem, TypePlace,
};
pub use crate::fact::Fact;
pub use crate::install_cx::{InstallCx, InstallFacts};
pub use crate::kind_spawn::{KindSpawn, SpawnCore};
pub use crate::kind_visitor::KindVisitor;
pub use crate::match_facts::MatchFacts;
pub use crate::module::{Module, ModuleVisitor};
pub use crate::module_systems::{ModuleSystems, NoSystems, SetPlace};
pub use crate::name_sets::NameSets;
pub use crate::named_scripts::{NamedScript, NamedScripts, ScriptParams};
pub use crate::named_texts::{NamedText, NamedTexts};
pub use crate::one_shot::OneShot;
pub use crate::per_id::PerId;
pub use crate::report::Report;
pub use crate::role::Role;
pub use crate::role_form::{ClientForm, RoleForm, ServerForm};
pub use crate::scratch::Scratch;
pub use crate::section::patched::Patched;
pub use crate::section::section_readers::{SectionReaders, TableRead, TableSeed};
pub use crate::section::section_writer::SectionWriter;
pub use crate::section::sections::Sections;
pub use crate::section::{Listed, ListedSet, ReadValue, Section, SectionTable};
pub use crate::server_only_systems::ServerOnlySystems;
pub use crate::spawn_parts::{PartWriter, SpawnParts};
pub use crate::state_fields::StateFields;
pub use crate::unit_defaults::UnitDefaults;
pub use crate::unit_type_data::{UnitTypeData, UnitTypeSeed};

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::book::internals::BookLoad;
}
