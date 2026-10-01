use std::iter;
use std::ops::Range;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::ScriptHost;
use campfire_script::rhai::ImmutableString;
use campfire_sim::{EntityIndex, Position, StableId};

use crate::actions::action_book::ActionId;
use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_kinds::SlotKinds;
use crate::combat::recent_attackers::RecentAttackers;
use crate::mode::choice_book::ChoiceBook;
use crate::mode::error::ModeError;
use crate::mode::game_map::GameMap;
use crate::mode::map_data::MapData;
use crate::mode::marker::{Marker, MarkerInfo};
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::{ModeSetup, SlotAction};
use crate::mode::roster::Roster;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::{PathEnd, PathWalker};
use crate::navigation::paths::Paths;
use crate::progression::experience::Experience;
use crate::scripts::frame::Frame;
use crate::stats::Stats;
use crate::stats::level::Level;
use crate::stats::modifier_book::{Applier, ModifierId};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifiers::Modifiers;
use crate::stats::unit_stats::UnitStats;
use crate::units::by_type::ByType;
use crate::units::path_id::PathId;
use crate::units::script_view::View;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct ModeBook {
    pub(crate) schema: ModeSchema,
    pub(crate) roster: Roster,
    pub(crate) teams: Rc<Teams>,
    pub(crate) bounds: Bounds,
    pub(crate) choices: ChoiceBook,
    pub(crate) slot_kinds: SlotKinds,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
    /// By unit type.
    kits: ByType<UnitKit>,
    passives: ByType<ModifierId>,
    /// Each unit type's actions, a run of `actions` each, kind after kind.
    action_runs: ByType<Range<usize>>,
    actions: Vec<SlotAction>,
    /// The units the map places from the start.
    pub(crate) placed: Vec<PlacedUnit>,
    /// `ctx.map`, as scripts read it, and where avatars spawn.
    pub(crate) map: GameMap,
}

/// A unit of the map, names resolved: on its path, if it names one, and walking it from `from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedUnit {
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) path: Option<PathId>,
    pub(crate) from: Option<PathEnd>,
    pub(crate) pos: Position,
}

impl ModeBook {
    /// The book of `setup`, which passed `Mode::check`, whose script `host` compiled, its names
    /// resolved through `view` and `paths`; an error when the teams have fewer slots than the
    /// players.
    pub(crate) fn new(
        setup: ModeSetup<'_>,
        host: &ScriptHost,
        view: &View,
        paths: &Paths,
    ) -> Result<ModeBook, ModeError> {
        let teams = setup
            .teams
            .iter()
            .map(|team| (team.name.as_str(), team.slots));
        let teams = Teams::new(teams, setup.players).ok_or(ModeError::TooManyPlayers)?;
        let (mut kits, mut passives, mut action_runs) =
            (ByType::default(), ByType::default(), ByType::default());
        let mut actions = Vec::new();
        for unit_type in &setup.unit_types {
            kits.set(unit_type.unit_type, unit_type.kit);
            if let Some(passive) = unit_type.passive {
                passives.set(unit_type.unit_type, passive);
            }
            let start = actions.len();
            actions.extend_from_slice(&unit_type.actions);
            action_runs.set(unit_type.unit_type, start..actions.len());
        }
        let mut book = ModeBook {
            schema: ModeSchema::new(setup.script, host, setup.data),
            roster: Roster::new(setup.avatars, setup.loadout),
            teams: Rc::new(teams),
            bounds: setup.map.bounds,
            choices: ChoiceBook::new(&setup.data.choices),
            slot_kinds: setup.data.slots.clone(),
            loadout_ranks: setup.data.loadout_ranks(),
            kits,
            passives,
            action_runs,
            actions,
            placed: Vec::new(),
            map: GameMap::default(),
        };
        book.set_map(setup.map, view, paths);
        Ok(book)
    }

    /// Resolves the names of `map`, which the check found, unit types through `view`: its paths,
    /// its placed units, and its markers, which `ctx.map` lists.
    fn set_map(&mut self, map: &MapData, view: &View, paths: &Paths) {
        let checked = "the mode's check passed";
        for unit in &map.units {
            let unit = PlacedUnit {
                unit_type: view.unit_type(&unit.unit_type).expect(checked),
                team: self.teams.named(&unit.team).expect(checked),
                path: unit
                    .path
                    .as_ref()
                    .map(|path| paths.named(path).expect(checked)),
                from: unit.from,
                pos: unit.pos.position().expect(checked),
            };
            self.placed.push(unit);
        }
        let markers = map.markers.iter().map(|marker| {
            let params = marker
                .params
                .iter()
                .map(|(name, param)| (name.as_str().into(), param.to_dynamic()));
            Marker::new(MarkerInfo {
                name: marker.name.as_str().into(),
                tags: marker.tags.iter().map(|tag| tag.as_str().into()).collect(),
                pos: marker.pos.map(|pos| pos.position().expect(checked)),
                team: marker
                    .team
                    .as_ref()
                    .map(|team| self.teams.named(team).expect(checked)),
                params: params.collect(),
            })
        });
        self.map = GameMap::new(paths.names().map(ImmutableString::from), markers);
    }

    /// The actions of `unit_type`, kind after kind.
    pub(crate) fn actions(&self, unit_type: UnitType) -> &[SlotAction] {
        self.action_runs
            .get(unit_type)
            .map_or(&[], |run| &self.actions[run.clone()])
    }

    pub(crate) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.kits.get(unit_type).copied()
    }

    pub(crate) fn map(&self) -> GameMap {
        self.map.clone()
    }

    /// Spawns `at`, with its kit, its actions, each at the first rank of its kind, its passive,
    /// whose params read through `frame` when a call spawned it, and `parts`.
    pub(crate) fn spawn(
        &self,
        world: &mut World,
        at: SpawnAt,
        parts: impl Bundle,
        frame: Option<&Frame>,
    ) -> Entity {
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
            parts,
        ));
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
            unit.insert(step.bundle());
        }
        if !kit.tracks.is_empty() {
            unit.insert(Experience::new(kit.tracks));
        }
        let actions = self.actions(unit_type);
        if !actions.is_empty() {
            let slots = actions.iter().map(|action| {
                let rank = self.slot_kinds.first_rank(action.kind);
                (action.ability, action.kind, rank)
            });
            unit.insert(ActionSlots::new(slots));
        }
        let entity = unit.id();
        if let Some(&passive) = self.passives.get(unit_type) {
            let applier = Applier {
                source: Some(id),
                ability: None,
                rank: 1,
                passive: true,
                held: false,
            };
            let add = ModifierEffect::Add {
                target: id,
                id: passive,
                duration: None,
            };
            Stats::apply_effect(world, add, applier, frame);
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
        frame: &Frame,
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
            self.spawn(world, at, walker, Some(frame));
        }
    }

    /// Puts `abilities` in `kind` of `unit`, after the slots of that kind it has, at the first
    /// rank of the kind; nothing for a unit that is gone.
    pub(crate) fn grant(
        &self,
        world: &mut World,
        unit: StableId,
        kind: SlotKind,
        abilities: &[ActionId],
    ) {
        let Some(entity) = world.resource::<EntityIndex>().get(unit) else {
            return;
        };
        let rank = self.slot_kinds.first_rank(kind);
        let mut unit = world.entity_mut(entity);
        if let Some(mut slots) = unit.get_mut::<ActionSlots>() {
            slots.grant(kind, abilities, rank);
        } else {
            let slots = abilities.iter().map(|&ability| (ability, kind, rank));
            unit.insert(ActionSlots::new(slots));
        }
    }
}

/// A unit to spawn: the id it takes, its unit type, its team and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpawnAt {
    pub(crate) id: StableId,
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) pos: Position,
}

/// A unit of a spawn group: its unit type, and the id a call took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupUnit {
    pub(crate) unit_type: UnitType,
    pub(crate) id: StableId,
}
