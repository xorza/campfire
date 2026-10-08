//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::mem;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::actions::action_book::ActionBook;
use crate::actions::actions_column::ActionsColumn;
use crate::actions::effect_lists::EffectLists;
use crate::areas::area_spec::AreaSpec;
use crate::books::book_builder::BookBuilder;
use crate::books::book_input::BookInput;
use crate::books::error::BookError;
use crate::items::item_book::ItemBook;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_units::ModeUnits;
use crate::navigation::error::MapProblem;
use crate::navigation::walker::Walker;
use crate::orders::ai::Ai;
use crate::production::build_specs::BuildSpecs;
use crate::production::node_book::NodeBook;
use crate::production::production_column::ProductionColumn;
use crate::production::production_data::ProductionData;
use crate::production::requirements::Requirements;
use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_data::SupplyData;
use crate::production::supply_rules::SupplyRules;
use crate::progression::progression_column::ProgressionColumn;
use crate::progression::track_book::TrackBook;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::ctx::Ctx;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::{ParamBook, ParamTables};
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::StatsColumn;
use crate::units::by_type::ByType;
use crate::units::predicting::Predicting;
use crate::units::script_view::View;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::units::units_column::UnitsColumn;
use crate::values::declared_name::DeclaredName;
use crate::vision::sight::Sight;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod error;
pub(crate) mod package_content;
pub(crate) mod type_place;
pub(crate) mod unit_type_file;

/// The books of a match, built once from its packages at its tick rate: what the builder loads
/// package by package, and the books of the mode's own rules, which read the unit types it
/// loaded.
#[derive(Debug)]
pub struct Books {
    parts: BookParts,
    mode: ModeBooks,
}

/// What the builder loads package by package: the unit types and tags, the tracks, the
/// modifiers and actions with their params and effect lists, the AIs, the projectile and area
/// specs and the delivery types' sights, and what the mode's book takes from them.
#[derive(Debug, Default)]
pub(crate) struct BookParts {
    types: UnitTypes,
    tracks: Option<TrackBook>,
    modifiers: ModifierBook,
    actions: ActionBook,
    /// Every action's and modifier's params.
    params: ParamTables,
    effects: EffectLists,
    ais: ByType<Ai>,
    projectiles: ByType<ProjectileSpec>,
    areas: ByType<AreaSpec>,
    /// The sights of the delivery types with a `vision` section.
    sights: ByType<Sight>,
    producers: ByType<ProductionData>,
    supplies: ByType<SupplyData>,
    requirements: Requirements,
    builds: BuildSpecs,
    nodes: NodeBook,
    /// The kind of walker of each unit type that walks.
    walkers: ByType<Walker>,
    supply_rules: Option<SupplyRules>,
    /// The mode's item types.
    items: ItemBook,
    units: ModeUnits,
}

/// What the mode's install takes from the books: its unit types, avatars and loadout, and the
/// books of its own rules.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub books: ModeBooks,
}

impl Books {
    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Books, BookError> {
        BookBuilder::new(input).build()
    }

    /// Checks that the mode's map can be walked by the kinds of its unit types that walk, as
    /// `ModeMap::check_walkable` says, among its placed units of the types that do not walk.
    pub fn check_walkable(&self) -> Result<(), MapProblem> {
        let kits = &self.parts.units.unit_types;
        let body_of = |unit_type| {
            let walks = self.parts.walkers.get(unit_type).is_some();
            let placed = kits.iter().find(|setup| setup.unit_type == unit_type);
            placed
                .expect("a placed unit's type stands")
                .kit
                .body
                .filter(|_| !walks)
        };
        let name_of = |unit_type: UnitType| {
            let name = self.parts.types.names().nth(unit_type.index());
            DeclaredName::new(name.expect("a type of the books"))
                .expect("a type's name is declared")
        };
        let walkers = &self.mode.walkers;
        self.mode.map.check_walkable(walkers, body_of, name_of)
    }

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled, each in the resource of the capability that reads it, and the view's and, with
    /// scripts, the frame's copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World) -> ModeInputs {
        let Books { parts, mode } = self;
        let view = world.non_send::<View>().clone();
        let states = parts.types.state_book();
        UnitsColumn::share(&view, states.clone());
        world.insert_resource(states);
        view.set_types(parts.types);
        StatsColumn::share_modifiers(&view, parts.modifiers.clone());
        let params = ParamBook::new(parts.params);
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            StatsCall::share_params(&mut ctx.frame(), params.clone());
        }
        world.insert_resource(params);
        if let Some(tracks) = parts.tracks {
            ProgressionColumn::share(&view, tracks.clone());
            world.insert_resource(tracks);
        }
        ActionsColumn::share(&view, parts.actions.clone());
        let costs = SupplyCosts::new(parts.supplies, &parts.actions);
        replace(world, parts.modifiers);
        replace(world, parts.actions);
        replace(world, parts.effects);
        replace(world, parts.ais);
        replace(world, parts.projectiles);
        replace(world, parts.areas);
        replace(world, parts.sights);
        replace(world, parts.producers);
        replace(world, parts.requirements);
        replace(world, parts.builds);
        replace(world, parts.nodes);
        replace(world, parts.walkers);
        ProductionColumn::share(&view, parts.supply_rules, costs.clone());
        replace(world, costs);
        if let Some(rules) = parts.supply_rules {
            world.insert_resource(rules);
        }
        replace(world, parts.items);
        ModeInputs {
            units: parts.units,
            books: mode,
        }
    }

    /// Puts the books in `world`, a client's, whose capabilities are installed with no scripts,
    /// and the part of the mode no script runs, as a match's mode install puts them: the books
    /// of its own rules, and its map's ground, its pathing grid for the kinds of its walkers. The
    /// client then predicts its units by the rules the server runs: it starts their actions, and
    /// runs none of their effects.
    pub fn install_prediction(self, world: &mut World) {
        let ModeInputs { mut books, .. } = self.install(world);
        let walkers = mem::take(&mut books.walkers);
        let ModeMap { ground, .. } = books.install(world);
        ground.install(world, walkers);
        world.insert_resource(Predicting);
    }
}

/// Puts `book` in place of the resource of its type in `world`, which the capability that reads
/// it installed; none when the match does not have that capability, whose book is then empty.
fn replace<T: Resource>(world: &mut World, book: T) {
    if world.contains_resource::<T>() {
        world.insert_resource(book);
    }
}
