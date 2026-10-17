use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

/// A unit type's `passive`: a modifier of its package it holds from its spawn, from itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct PassiveData(pub DeclaredName);

impl Section for PassiveData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "passive";
}
