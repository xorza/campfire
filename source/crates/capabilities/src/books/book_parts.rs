use campfire_actions::{ActionBook, ActionSlots, EffectListBook, UseGroup};
use campfire_areas::AreaSpec;
use campfire_combat::{Armor, OnDeath, Turret};
use campfire_declare::ByType;
use campfire_interaction::{UseSlot, UseSpec};
use campfire_items::{Inventory, ItemBook, Shop};
use campfire_mode::ModeUnits;
use campfire_navigation::Walker;
use campfire_orders::Ai;
use campfire_production::{
    BuildBook, Builder, Gatherer, ModeSupplyData, Node, NodeBook, ProductionData, RequirementBook,
    SupplyData,
};
use campfire_progression::{TrackBook, TrackSet};
use campfire_projectiles::ProjectileSpec;
use campfire_stats::{Life, ModifierBook, ParamTables, Pools, TypePassive};
use campfire_units::{BodyForm, MoveStep, TurnStep, UnitTypes};
use campfire_vision::Sight;

/// What the builder loads package by package: the unit types and tags, the tracks, the
/// modifiers and actions with their params and effect lists, the AIs, the projectile and area
/// specs, what each unit type spawns with, and what the mode's book takes from them.
#[derive(Debug, Default)]
pub(crate) struct BookParts {
    pub(super) types: UnitTypes,
    pub(super) tracks: TrackBook,
    pub(super) modifiers: ModifierBook,
    pub(super) actions: ActionBook,
    /// Every action's and modifier's params.
    pub(super) params: ParamTables,
    pub(super) effects: EffectListBook,
    pub(super) ais: ByType<Ai>,
    pub(super) projectiles: ByType<ProjectileSpec>,
    pub(super) areas: ByType<AreaSpec>,
    /// The sights of the unit types with a `vision` section, a delivery type's among them.
    pub(super) sights: ByType<Sight>,
    pub(super) lives: ByType<Life>,
    /// The pools but the life of each type with any.
    pub(super) pools: ByType<Pools>,
    pub(super) passives: ByType<TypePassive>,
    pub(super) deaths: ByType<OnDeath>,
    pub(super) armors: ByType<Armor>,
    pub(super) steps: ByType<MoveStep>,
    pub(super) turns: ByType<TurnStep>,
    pub(super) turrets: ByType<Turret>,
    pub(super) bodies: ByType<BodyForm>,
    /// The tracks of each type that gains experience.
    pub(super) type_tracks: ByType<TrackSet>,
    pub(super) builders: ByType<Builder>,
    /// What each node holds as its unit spawns.
    pub(super) node_starts: ByType<Node>,
    pub(super) gatherers: ByType<Gatherer>,
    pub(super) inventories: ByType<Inventory>,
    pub(super) use_groups: ByType<UseGroup>,
    pub(super) shops: ByType<Shop>,
    /// The use of each object type.
    pub(super) use_specs: ByType<UseSpec>,
    /// The use slot of each user type.
    pub(super) use_slots: ByType<UseSlot>,
    /// The slots each type with actions, an inventory or a use slot spawns with.
    pub(super) slots: ByType<ActionSlots>,
    pub(super) producers: ByType<ProductionData>,
    pub(super) supplies: ByType<SupplyData>,
    pub(super) requirements: RequirementBook,
    pub(super) builds: BuildBook,
    pub(super) nodes: NodeBook,
    /// The kind of walker of each unit type that walks.
    pub(super) walkers: ByType<Walker>,
    pub(super) supply: Option<ModeSupplyData>,
    /// The mode's item types.
    pub(super) items: ItemBook,
    pub(super) units: ModeUnits,
}
