use std::iter;
use std::ops::Range;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{EntityIndex, StableId};

use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::combat::recent_attackers::RecentAttackers;
use crate::items::inventory::Inventory;
use crate::mode::choice_book::ChoiceBook;
use crate::mode::game_map::GameMap;
use crate::mode::group_unit::GroupUnit;
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::{ModeSetup, SlotAction};
use crate::mode::placed_unit::PlacedUnit;
use crate::mode::roster::Roster;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::Navigation;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::{PathEnd, PathWalker};
use crate::navigation::paths::Paths;
use crate::production::train_queue::TrainQueue;
use crate::progression::experience::Experience;
use crate::progression::points::Points;
use crate::progression::track_book::TrackBook;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_book::ScriptBook;
use crate::stats::Stats;
use crate::stats::applier::Applier;
use crate::stats::level::Level;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::Modifiers;
use crate::stats::stats_effect::StatsEffect;
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::spawn_point::SpawnPoint;
use crate::units::spawner::SpawnAt;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_type::UnitType;

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct ModeBook {
    pub(crate) schema: ModeSchema,
    pub(crate) roster: Roster,
    pub(crate) teams: Rc<Teams>,
    pub(crate) choices: ChoiceBook,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
    /// What each of the mode's unit types spawns with.
    types: ByType<ModeType>,
    /// The unit types' actions, a run of each, kind after kind.
    actions: Vec<SlotAction>,
    /// The units the map places from the start.
    pub(crate) placed: Vec<PlacedUnit>,
    /// `ctx.map`, as scripts read it, and where avatars spawn.
    map: GameMap,
}

/// What a unit type of the mode spawns with: its kit, its passive, if it holds one, and its run of
/// the book's actions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModeType {
    kit: UnitKit,
    passive: Option<ModifierId>,
    actions: Range<usize>,
}

impl ModeBook {
    /// The mode of the match `ctx` runs in, once it installed.
    pub(crate) fn of(ctx: &Ctx) -> Option<&ModeBook> {
        ctx.mode()?.downcast_ref()
    }

    /// The mode of the match `ctx` runs in; an error in a match with none.
    pub(crate) fn of_or_fail(ctx: &Ctx) -> Checked<&ModeBook> {
        ModeBook::of(ctx).ok_or_else(|| ApiError::NoMode.fail().into())
    }

    /// The book of `setup`, whose script defines the hooks `scripts` gives, for players the teams
    /// seat, with the units its map places and `map` as scripts read it.
    pub(crate) fn new(
        setup: ModeSetup<'_>,
        scripts: &ScriptBook,
        placed: Vec<PlacedUnit>,
        map: GameMap,
    ) -> ModeBook {
        let teams = setup
            .teams
            .iter()
            .map(|team| (team.name.as_str(), team.slots));
        let teams = Teams::new(teams, setup.players)
            .expect("the session checked its players against the teams' slots");
        let mut types = ByType::default();
        let mut actions = Vec::new();
        for unit_type in &setup.units.unit_types {
            let start = actions.len();
            actions.extend_from_slice(&unit_type.actions);
            let held = ModeType {
                kit: unit_type.kit,
                passive: unit_type.passive,
                actions: start..actions.len(),
            };
            types.set(unit_type.unit_type, held);
        }
        ModeBook {
            schema: ModeSchema::new(setup.script, scripts, setup.data),
            roster: Roster::new(setup.units.avatars, &setup.units.loadout),
            teams: Rc::new(teams),
            choices: ChoiceBook::new(&setup.data.choices),
            loadout_ranks: setup.data.loadout_ranks(),
            types,
            actions,
            placed,
            map,
        }
    }

    /// The actions of `unit_type`, kind after kind.
    pub(crate) fn actions(&self, unit_type: UnitType) -> &[SlotAction] {
        self.types
            .get(unit_type)
            .map_or(&[], |held| &self.actions[held.actions.clone()])
    }

    pub(super) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.types.get(unit_type).map(|held| held.kit)
    }

    pub(crate) fn map(&self) -> GameMap {
        self.map.clone()
    }

    /// Spawns `at` as `spawn` does, owned by `owner` when it names a player.
    pub(crate) fn spawn_owned(
        &self,
        world: &mut World,
        at: SpawnAt,
        owner: Option<PlayerSlot>,
    ) -> Entity {
        match owner {
            Some(slot) => self.spawn(world, at, Owner::new(slot)),
            None => self.spawn(world, at, ()),
        }
    }

    /// Spawns `at`, with its kit, its script state at its type's defaults, its actions, each at
    /// the first rank of its kind, an empty slot of its inventory's kind for each inventory slot,
    /// its passive, and `parts`.
    pub(crate) fn spawn(&self, world: &mut World, at: SpawnAt, parts: impl Bundle) -> Entity {
        let SpawnAt {
            id,
            unit_type,
            team,
            pos,
        } = at;
        let kit = self
            .kit(unit_type)
            .expect("a unit type of the mode has a kit");
        let tags = world
            .resource::<TagBook>()
            .unit_tags(unit_type, iter::empty());
        let level_track = world
            .get_resource::<TrackBook>()
            .and_then(TrackBook::level_track);
        let state = world.resource::<UnitStateBook>().initial(unit_type);
        let mut unit = world.spawn((
            id,
            pos,
            SpawnPoint::new(pos),
            unit_type,
            team,
            Level::default(),
            UnitStats::default(),
            tags,
            Modifiers::default(),
            ModifierClocks::default(),
            parts,
        ));
        if let Some(state) = state {
            unit.insert(state);
        }
        if let Some(pools) = kit.pools {
            unit.insert(pools);
        }
        if let Some(on_death) = kit.on_death {
            unit.insert((on_death, RecentAttackers::default()));
        }
        if let Some(sight) = kit.sight {
            unit.insert(sight);
        }
        if let Some(body) = kit.body {
            unit.insert(body);
        }
        if let Some(step) = kit.step {
            unit.insert(Navigation::walker(step));
        }
        if !kit.tracks.is_empty() {
            unit.insert(Experience::new(kit.tracks, level_track));
        }
        if let Some(points) = Points::at_spawn(kit.tracks, level_track) {
            unit.insert(points);
        }
        if kit.queue.is_some() {
            unit.insert(TrainQueue::default());
        }
        let actions = self.actions(unit_type);
        if !actions.is_empty() || kit.inventory.is_some() {
            let slots = actions
                .iter()
                .map(|action| (action.ability, action.kind, action.rank));
            let mut slots = ActionSlots::new(slots);
            if let Some(inventory) = kit.inventory {
                slots.add_empty(inventory.kind, inventory.slots.get());
                unit.insert(Inventory::new(inventory.slots, inventory.kind));
            }
            unit.insert(slots);
        }
        let entity = unit.id();
        if let Some(passive) = self.types.get(unit_type).and_then(|held| held.passive) {
            let applier = Applier {
                source: Some(id),
                ability: None,
                rank: 1,
                hold: Some(Hold::Passive),
            };
            let add = StatsEffect::Add {
                target: id,
                id: passive,
                duration: None,
            };
            Stats::apply_effect(world, add, applier);
        }
        entity
    }

    /// Spawns `units` of `team`, each a unit type with the id a call took for it, in order at the
    /// end `from` of `path`, walking it from there.
    pub(crate) fn spawn_group(
        &self,
        world: &mut World,
        team: Team,
        path: PathId,
        from: PathEnd,
        units: &[GroupUnit],
    ) {
        let pos = world
            .resource::<Paths>()
            .waypoint(path, 0, from)
            .expect("a path has a waypoint");
        for &GroupUnit { unit_type, id } in units {
            let walker = (OnPath::new(path), PathWalker::start(from));
            let at = SpawnAt {
                id,
                unit_type,
                team,
                pos,
            };
            self.spawn(world, at, walker);
        }
    }

    /// Puts `abilities` in `kind` of `unit`, after the slots of that kind it has, at the first
    /// rank of the kind; nothing for a unit that is gone.
    pub(crate) fn grant(
        world: &mut World,
        unit: StableId,
        kind: SlotKind,
        rank: u8,
        abilities: &[ActionId],
    ) {
        let Some(entity) = world.resource::<EntityIndex>().get(unit) else {
            return;
        };
        let mut unit = world.entity_mut(entity);
        if let Some(mut slots) = unit.get_mut::<ActionSlots>() {
            slots.grant(kind, abilities, rank);
        } else {
            let slots = abilities.iter().map(|&ability| (ability, kind, rank));
            unit.insert(ActionSlots::new(slots));
        }
    }
}
