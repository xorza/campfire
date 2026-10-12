use std::collections::BTreeMap;

use serde::Deserialize;

use crate::areas::area_data::AreaData;
use crate::combat::combat_data::CombatData;
use crate::items::inventory_data::InventoryData;
use crate::orders::ai_data::AiData;
use crate::production::drop_off_data::DropOffData;
use crate::production::node_data::NodeData;
use crate::production::production_data::ProductionData;
use crate::production::supply_data::SupplyData;
use crate::projectiles::projectile_data::ProjectileData;
use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::stats::stats_data::StatsData;
use crate::units::collision_data::CollisionData;
use crate::units::unit_type_core::UnitTypeCore;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;
use crate::values::stat::{EngineStat, Stat};
use crate::vision::vision_data::VisionData;

/// A unit type as its data file declares it, in one table: the core's tags, params and state,
/// and a section for each capability the type uses, its pools beside its stats, its actions by
/// slot kind and the modifier it holds from its spawn.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitTypeData {
    #[serde(default)]
    pub tags: Vec<DeclaredName>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, Scalar>,
    #[serde(default)]
    pub state: BTreeMap<DeclaredName, SyncedStateDecl>,
    #[serde(default)]
    pub pools: Vec<DeclaredName>,
    /// Its package's actions in each slot kind, in order.
    #[serde(default)]
    pub slots: BTreeMap<DeclaredName, Vec<DeclaredName>>,
    /// A modifier of its package it holds from its spawn, from itself.
    pub passive: Option<DeclaredName>,
    pub stats: Option<StatsData>,
    pub combat: Option<CombatData>,
    pub orders: Option<AiData>,
    pub vision: Option<VisionData>,
    pub collision: Option<CollisionData>,
    /// The mode's tracks it gains experience on.
    #[serde(default)]
    pub tracks: Vec<DeclaredName>,
    pub production: Option<ProductionData>,
    pub supply: Option<SupplyData>,
    /// It is a node workers gather from.
    pub node: Option<NodeData>,
    /// Workers' loads of its resources join their player's there.
    pub drop_off: Option<DropOffData>,
    /// It is a projectile type: actions deliver its units.
    pub projectile: Option<ProjectileData>,
    /// It is an area type: actions deliver its units.
    pub area: Option<AreaData>,
    /// The slots it carries items in.
    pub inventory: Option<InventoryData>,
}

impl UnitTypeData {
    /// Its core fields, as the match's unit types load them.
    pub fn core(&self) -> UnitTypeCore {
        UnitTypeCore {
            tags: self.tags.clone(),
            params: self.params.clone(),
            state: self.state.clone(),
        }
    }

    /// Whether its units walk: its stats declare a move speed.
    pub fn walks(&self) -> bool {
        let move_speed = Stat::Engine(EngineStat::MoveSpeed);
        self.stats
            .as_ref()
            .is_some_and(|stats| stats.declares(&move_speed))
    }

    /// Whether it is a delivery type: a `projectile` or an `area` section makes it one, which
    /// no map or train places.
    pub const fn delivers(&self) -> bool {
        self.projectile.is_some() || self.area.is_some()
    }

    /// The fields of its table, by name, in the order `given` gives them.
    pub const FIELDS: [&str; 19] = [
        "tags",
        "params",
        "state",
        "pools",
        "slots",
        "passive",
        "stats",
        "combat",
        "orders",
        "vision",
        "collision",
        "tracks",
        "production",
        "supply",
        "node",
        "drop_off",
        "projectile",
        "area",
        "inventory",
    ];

    /// The name of each field its table gives, a list or map one only when it is not empty.
    pub fn given(&self) -> impl Iterator<Item = &'static str> {
        let UnitTypeData {
            tags,
            params,
            state,
            pools,
            slots,
            passive,
            stats,
            combat,
            orders,
            vision,
            collision,
            tracks,
            production,
            supply,
            node,
            drop_off,
            projectile,
            area,
            inventory,
        } = self;
        let given = [
            !tags.is_empty(),
            !params.is_empty(),
            !state.is_empty(),
            !pools.is_empty(),
            !slots.is_empty(),
            passive.is_some(),
            stats.is_some(),
            combat.is_some(),
            orders.is_some(),
            vision.is_some(),
            collision.is_some(),
            !tracks.is_empty(),
            production.is_some(),
            supply.is_some(),
            node.is_some(),
            drop_off.is_some(),
            projectile.is_some(),
            area.is_some(),
            inventory.is_some(),
        ];
        UnitTypeData::FIELDS
            .into_iter()
            .zip(given)
            .filter_map(|(name, given)| given.then_some(name))
    }

    /// Whether it is a delivery type and nothing more: a `projectile` or an `area` section, one
    /// of them, beside its tags, params, state and `vision`, and no section of a unit that
    /// stands.
    pub fn delivery_only(&self) -> bool {
        let delivery = ["tags", "params", "state", "vision", "projectile", "area"];
        self.projectile.is_some() != self.area.is_some()
            && self.given().all(|field| delivery.contains(&field))
    }
}

impl DataSection for UnitTypeData {
    const TABLE: DataTable = DataTable::UnitType;
}
