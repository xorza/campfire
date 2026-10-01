use std::collections::BTreeMap;

use campfire_capabilities::{
    AiData, CollisionData, CombatData, DeclaredName, Scalar, StatsData, UnitTypeData, VisionData,
};
use serde::{Deserialize, Deserializer};

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitsData {
    pub units: BTreeMap<String, UnitTypeFile>,
}

/// A unit type as its data file declares it: the core's tags and params, and a section for each
/// capability the type uses, in one table, its pools beside its stats, its actions by slot kind
/// and the modifier it holds from its spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeFile {
    pub core: UnitTypeData,
    pub pools: Vec<DeclaredName>,
    /// Its package's actions in each slot kind, in order.
    pub slots: BTreeMap<DeclaredName, Vec<String>>,
    /// A modifier of its package it holds from its spawn, from itself.
    pub passive: Option<String>,
    pub stats: Option<StatsData>,
    pub combat: Option<CombatData>,
    pub orders: Option<AiData>,
    pub vision: Option<VisionData>,
    pub collision: Option<CollisionData>,
    /// The mode's tracks it gains experience on.
    pub tracks: Vec<DeclaredName>,
}

/// The flat table of a unit type, the core's fields among the capabilities' sections.
impl<'de> Deserialize<'de> for UnitTypeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UnitTypeFile, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            tags: Vec<String>,
            #[serde(default)]
            params: BTreeMap<String, Scalar>,
            #[serde(default)]
            pools: Vec<DeclaredName>,
            #[serde(default)]
            slots: BTreeMap<DeclaredName, Vec<String>>,
            passive: Option<String>,
            stats: Option<StatsData>,
            combat: Option<CombatData>,
            orders: Option<AiData>,
            vision: Option<VisionData>,
            collision: Option<CollisionData>,
            #[serde(default)]
            tracks: Vec<DeclaredName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(UnitTypeFile {
            core: UnitTypeData {
                tags: fields.tags,
                params: fields.params,
            },
            pools: fields.pools,
            slots: fields.slots,
            passive: fields.passive,
            stats: fields.stats,
            combat: fields.combat,
            orders: fields.orders,
            vision: fields.vision,
            collision: fields.collision,
            tracks: fields.tracks,
        })
    }
}
