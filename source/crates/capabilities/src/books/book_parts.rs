use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::effect_list_book::EffectListBook;
use crate::actions::use_group::UseGroup;
use crate::areas::area_spec::AreaSpec;
use crate::combat::on_death::OnDeath;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::mode::mode_units::ModeUnits;
use crate::navigation::walker::Walker;
use crate::orders::ai::Ai;
use crate::production::build_book::BuildBook;
use crate::production::builder::Builder;
use crate::production::gatherer::Gatherer;
use crate::production::mode_supply_data::ModeSupplyData;
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::production_data::ProductionData;
use crate::production::requirement_book::RequirementBook;
use crate::production::supply_data::SupplyData;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::stats::life::Life;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::ParamTables;
use crate::stats::pools::Pools;
use crate::units::body_form::BodyForm;
use crate::units::by_type::ByType;
use crate::units::move_step::MoveStep;
use crate::units::unit_types::UnitTypes;
use crate::vision::sight::Sight;

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
    pub(super) deaths: ByType<OnDeath>,
    pub(super) steps: ByType<MoveStep>,
    pub(super) bodies: ByType<BodyForm>,
    /// The tracks of each type that gains experience.
    pub(super) type_tracks: ByType<TrackSet>,
    pub(super) builders: ByType<Builder>,
    /// What each node holds as its unit spawns.
    pub(super) node_starts: ByType<Node>,
    pub(super) gatherers: ByType<Gatherer>,
    pub(super) inventories: ByType<Inventory>,
    pub(super) use_groups: ByType<UseGroup>,
    /// The slots each type with actions or an inventory spawns with.
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
