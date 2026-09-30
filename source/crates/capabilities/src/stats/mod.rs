//! The `stats` capability, for now its data: a unit type's base stats and how they grow by level,
//! and modifiers as their data declares them. How stats combine under modifiers comes later.

pub(crate) mod modifier_data;
pub(crate) mod stat;
pub(crate) mod stats_data;
pub(crate) mod unit_state;
