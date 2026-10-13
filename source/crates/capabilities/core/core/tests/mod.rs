//! The tests of the crate that build a whole match, which links the crate once.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod players_players_world;
mod scripts_script_api;
mod scripts_script_facts;
mod units;
mod units_entity_kinds;
mod units_status_tags;
mod units_tag_book;
mod units_unit_grid;
mod units_unit_types;
