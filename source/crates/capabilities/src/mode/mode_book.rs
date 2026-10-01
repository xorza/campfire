use std::iter;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::PlayerSlot;
use campfire_script::ScriptHost;
use campfire_script::rhai::ImmutableString;
use campfire_sim::{IdAllocator, Position, StableId};

use crate::abilities::ability_slots::AbilitySlots;
use crate::mode::error::ModeError;
use crate::mode::game_map::GameMap;
use crate::mode::map_data::MapData;
use crate::mode::marker::{Marker, MarkerInfo};
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::{ModeSetup, UnitTypeSetup};
use crate::mode::picks::Picks;
use crate::mode::roster::Roster;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::{PathEnd, PathWalker};
use crate::navigation::paths::Paths;
use crate::scripts::frame::Frame;
use crate::stats::Stats;
use crate::stats::level::Level;
use crate::stats::modifier_book::Applier;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifiers::Modifiers;
use crate::stats::unit_stats::UnitStats;
use crate::units::by_type::ByType;
use crate::units::owner::Owner;
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
    /// By unit type.
    kits: ByType<UnitKit>,
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
        let mut kits = ByType::default();
        for &UnitTypeSetup { unit_type, kit, .. } in &setup.unit_types {
            kits.set(unit_type, kit);
        }
        let mut book = ModeBook {
            schema: ModeSchema::new(setup.script, host, setup.data),
            roster: Roster::new(setup.avatars, setup.loadout),
            teams: Rc::new(teams),
            bounds: setup.map.bounds,
            kits,
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

    pub(crate) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.kits.get(unit_type).copied()
    }

    pub(crate) fn map(&self) -> GameMap {
        self.map.clone()
    }

    /// Spawns a unit of `unit_type` on `team` at `pos`, with its kit and `parts`.
    pub(crate) fn spawn(
        &self,
        world: &mut World,
        unit_type: UnitType,
        team: Team,
        pos: Position,
        parts: impl Bundle,
    ) -> Entity {
        let id: StableId = world.resource_mut::<IdAllocator>().allocate();
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
        if let Some(combatant) = kit.combatant {
            combatant.insert(&mut unit);
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
        unit.id()
    }

    /// Spawns the avatar of each player who chose one and has none yet, in slot order, at the
    /// point of their team's marker with tag `tag`, which the call checked each playing team has:
    /// under their control, with its abilities unlearned and their loadout, its passive's params
    /// read through `frame`.
    pub(crate) fn spawn_avatars(&self, world: &mut World, frame: &Frame, tag: &str) {
        for slot in (0..self.teams.players()).map(PlayerSlot::new) {
            let pick = world.resource::<Picks>().of(slot);
            let (Some(avatar), false) = (pick.avatar, pick.spawned) else {
                continue;
            };
            let loadout = pick.loadout.clone();
            let avatar = self.roster.avatar_setup(avatar);
            let team = self.teams.of(slot).expect("a player has a team");
            let own = avatar.abilities.iter().map(|&ability| (ability, 0));
            let chosen = loadout
                .iter()
                .map(|&entry| (self.roster.loadout_ability(entry), 1));
            let slots = AbilitySlots::new(own.chain(chosen));
            let entity = self.spawn(
                world,
                avatar.unit_type,
                team,
                self.map
                    .point(tag, team)
                    .expect("the call checked each team's spawn"),
                (Owner::new(slot), slots),
            );
            if let Some(passive) = avatar.passive {
                let id = *world
                    .get::<StableId>(entity)
                    .expect("a spawned unit has an id");
                let applier = Applier {
                    source: Some(id),
                    ability: None,
                    rank: 1,
                    passive: true,
                    aura: false,
                };
                let add = ModifierEffect::Add {
                    target: id,
                    id: passive,
                    duration: None,
                };
                Stats::apply_effect(world, add, applier, Some(frame));
            }
            world.resource_mut::<Picks>().of_mut(slot).spawned = true;
        }
    }

    /// Spawns `types` of `team` in order at the end `from` of `path`, walking it from there.
    pub(crate) fn spawn_group(
        &self,
        world: &mut World,
        team: Team,
        path: PathId,
        from: PathEnd,
        types: &[UnitType],
    ) {
        let start = world
            .resource::<Paths>()
            .waypoint(path, 0, from)
            .expect("a path has a waypoint");
        for &unit_type in types {
            let walker = (OnPath::new(path), PathWalker::start(from));
            self.spawn(world, unit_type, team, start, walker);
        }
    }
}
