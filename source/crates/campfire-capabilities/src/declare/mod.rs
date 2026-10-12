//! How the engine's modules declare what they add to a match, and what a module's install reads:
//! the declared capabilities, the declarations each module makes once, and the books.

pub(crate) mod book;
pub(crate) mod cache;
pub(crate) mod capability_set;
pub(crate) mod declarations;
pub(crate) mod error;
pub(crate) mod install_cx;
pub(crate) mod kind_visitor;
pub(crate) mod module;
pub(crate) mod parted_kind;
pub(crate) mod per_type;
pub(crate) mod report;
pub(crate) mod scratch;
