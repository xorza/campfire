use crate::actions::action_book::ActionBook;
use crate::actions::effect_lists::EffectLists;
use crate::areas::area_spec::AreaSpec;
use crate::items::item_book::ItemBook;
use crate::mode::mode_units::ModeUnits;
use crate::navigation::walker::Walker;
use crate::orders::ai::Ai;
use crate::production::build_specs::BuildSpecs;
use crate::production::node_book::NodeBook;
use crate::production::production_data::ProductionData;
use crate::production::requirements::Requirements;
use crate::production::supply_data::SupplyData;
use crate::production::supply_rules::SupplyRules;
use crate::progression::track_book::TrackBook;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::ParamTables;
use crate::units::by_type::ByType;
use crate::units::unit_types::UnitTypes;
use crate::vision::sight::Sight;

/// What the builder loads package by package: the unit types and tags, the tracks, the
/// modifiers and actions with their params and effect lists, the AIs, the projectile and area
/// specs and the delivery types' sights, and what the mode's book takes from them.
#[derive(Debug, Default)]
pub(crate) struct BookParts {
    pub(super) types: UnitTypes,
    pub(super) tracks: Option<TrackBook>,
    pub(super) modifiers: ModifierBook,
    pub(super) actions: ActionBook,
    /// Every action's and modifier's params.
    pub(super) params: ParamTables,
    pub(super) effects: EffectLists,
    pub(super) ais: ByType<Ai>,
    pub(super) projectiles: ByType<ProjectileSpec>,
    pub(super) areas: ByType<AreaSpec>,
    /// The sights of the delivery types with a `vision` section.
    pub(super) sights: ByType<Sight>,
    pub(super) producers: ByType<ProductionData>,
    pub(super) supplies: ByType<SupplyData>,
    pub(super) requirements: Requirements,
    pub(super) builds: BuildSpecs,
    pub(super) nodes: NodeBook,
    /// The kind of walker of each unit type that walks.
    pub(super) walkers: ByType<Walker>,
    pub(super) supply_rules: Option<SupplyRules>,
    /// The mode's item types.
    pub(super) items: ItemBook,
    pub(super) units: ModeUnits,
}
