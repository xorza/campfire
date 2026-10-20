use campfire_declarations::{Section, SectionTable};
use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

/// A unit type's `node` section: a node of the mode's player resource `resource`, holding
/// `amount` as it spawns, and what becomes of it once it runs out, `goes` when absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeData {
    pub resource: DeclaredName,
    pub amount: NonZeroU32,
    #[serde(default)]
    pub empty: EmptyNode,
}

/// What becomes of a node that ran out (04-capabilities/production.md, An empty node may stay).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyNode {
    /// It despawns as the tick it ran out in ends.
    #[default]
    Goes,
    /// It stands, and no worker gathers it.
    Stays,
}

impl DataSection for NodeData {
    const TABLE: DataTable = DataTable::new("A unit type's `node`");
}

impl Section for NodeData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "node";
}
