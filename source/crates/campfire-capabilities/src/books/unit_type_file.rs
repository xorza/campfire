use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::areas::area_data::AreaData;
use crate::combat::combat_data::CombatData;
use crate::orders::ai_data::AiData;
use crate::production::production_data::ProductionData;
use crate::projectiles::projectile_data::ProjectileData;
use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::stats::stats_data::StatsData;
use crate::units::collision_data::CollisionData;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;
use crate::values::stat::{EngineStat, Stat};
use crate::vision::vision_data::VisionData;

/// A unit type as its data file declares it: the core's tags and params, and a section for each
/// capability the type uses, in one table, its pools beside its stats, its actions by slot kind
/// and the modifier it holds from its spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeFile {
    pub core: UnitTypeData,
    pub pools: Vec<DeclaredName>,
    /// Its package's actions in each slot kind, in order.
    pub slots: BTreeMap<DeclaredName, Vec<DeclaredName>>,
    /// A modifier of its package it holds from its spawn, from itself.
    pub passive: Option<DeclaredName>,
    pub stats: Option<StatsData>,
    pub combat: Option<CombatData>,
    pub orders: Option<AiData>,
    pub vision: Option<VisionData>,
    pub collision: Option<CollisionData>,
    /// The mode's tracks it gains experience on.
    pub tracks: Vec<DeclaredName>,
    pub production: Option<ProductionData>,
    /// It is a projectile type: actions deliver its units.
    pub projectile: Option<ProjectileData>,
    /// It is an area type: actions deliver its units.
    pub area: Option<AreaData>,
}

impl UnitTypeFile {
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

    /// Whether it is a delivery type and nothing more: a `projectile` or an `area` section, one
    /// of them, beside its tags, params and state, and no section of a unit that stands.
    pub fn delivery_only(&self) -> bool {
        let UnitTypeFile {
            core: _,
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
            projectile,
            area,
        } = self;
        projectile.is_some() != area.is_some()
            && pools.is_empty()
            && slots.is_empty()
            && passive.is_none()
            && stats.is_none()
            && combat.is_none()
            && orders.is_none()
            && vision.is_none()
            && collision.is_none()
            && tracks.is_empty()
            && production.is_none()
    }
}

/// The flat table of a unit type, the core's fields among the capabilities' sections.
impl<'de> Deserialize<'de> for UnitTypeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UnitTypeFile, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            tags: Vec<DeclaredName>,
            #[serde(default)]
            params: BTreeMap<DeclaredName, Scalar>,
            #[serde(default)]
            state: BTreeMap<DeclaredName, SyncedStateDecl>,
            #[serde(default)]
            pools: Vec<DeclaredName>,
            #[serde(default)]
            slots: BTreeMap<DeclaredName, Vec<DeclaredName>>,
            passive: Option<DeclaredName>,
            stats: Option<StatsData>,
            combat: Option<CombatData>,
            orders: Option<AiData>,
            vision: Option<VisionData>,
            collision: Option<CollisionData>,
            #[serde(default)]
            tracks: Vec<DeclaredName>,
            production: Option<ProductionData>,
            projectile: Option<ProjectileData>,
            area: Option<AreaData>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(UnitTypeFile {
            core: UnitTypeData {
                tags: fields.tags,
                params: fields.params,
                state: fields.state,
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
            production: fields.production,
            projectile: fields.projectile,
            area: fields.area,
        })
    }
}
