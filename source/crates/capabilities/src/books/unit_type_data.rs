use std::collections::BTreeMap;

use campfire_areas::AreaData;
use campfire_combat::{CombatData, TurretData};
use campfire_common::DeclaredName;
use campfire_interaction::{UseData, UsesData};
use campfire_items::{InventoryData, ShopData};
use campfire_orders::AiData;
use campfire_production::{DropOffData, NodeData, ProductionData, SupplyData};
use campfire_projectiles::ProjectileData;
use campfire_script::{DataSection, DataTable, SyncedStateDecl};
use campfire_stats::StatsData;
use campfire_units::{CollisionData, UnitTypeCore};
use campfire_values::{EngineStat, Scalar, Stat};
use campfire_vision::VisionData;
use serde::Deserialize;

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
    /// The turret its attacks of a slot kind turn with.
    pub turret: Option<TurretData>,
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
    /// It is a shop: its units sell items.
    pub shop: Option<ShopData>,
    /// It is an object: units use it.
    #[serde(rename = "use")]
    pub usable: Option<UseData>,
    /// Its units use objects.
    pub uses: Option<UsesData>,
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

    /// Each field of its table by name, in the table's order, and whether it gives it: a list or
    /// a map only when it is not empty.
    pub fn fields(&self) -> [(&'static str, bool); 23] {
        let Self {
            tags,
            params,
            state,
            pools,
            slots,
            passive,
            stats,
            combat,
            turret,
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
            shop,
            usable,
            uses,
        } = self;
        [
            ("tags", !tags.is_empty()),
            ("params", !params.is_empty()),
            ("state", !state.is_empty()),
            ("pools", !pools.is_empty()),
            ("slots", !slots.is_empty()),
            ("passive", passive.is_some()),
            ("stats", stats.is_some()),
            ("combat", combat.is_some()),
            ("turret", turret.is_some()),
            ("orders", orders.is_some()),
            ("vision", vision.is_some()),
            ("collision", collision.is_some()),
            ("tracks", !tracks.is_empty()),
            ("production", production.is_some()),
            ("supply", supply.is_some()),
            ("node", node.is_some()),
            ("drop_off", drop_off.is_some()),
            ("projectile", projectile.is_some()),
            ("area", area.is_some()),
            ("inventory", inventory.is_some()),
            ("shop", shop.is_some()),
            ("use", usable.is_some()),
            ("uses", uses.is_some()),
        ]
    }

    /// The name of each field its table gives, as `fields` tells.
    pub fn given(&self) -> impl Iterator<Item = &'static str> {
        self.fields()
            .into_iter()
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
