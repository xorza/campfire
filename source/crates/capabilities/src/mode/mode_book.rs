use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::ScriptHost;
use campfire_script::rhai::{Array, Dynamic, ImmutableString, Map};
use campfire_sim::{IdAllocator, PlayerSlot, Position, StableId, TickRate};

use crate::abilities::ability_slots::AbilitySlots;
use crate::mode::error::ModeError;
use crate::mode::map_data::MapData;
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::{ModeSetup, UnitTypeSetup};
use crate::mode::picks::Picks;
use crate::mode::roster::Roster;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::lane_walker::{LaneWalker, PathDirection};
use crate::navigation::lanes::Lanes;
use crate::navigation::on_lane::OnLane;
use crate::scripts::error::ApiError;
use crate::units::by_type::ByType;
use crate::units::lane::Lane;
use crate::units::owner::Owner;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::unit_type::UnitType;

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct ModeBook {
    pub(crate) rate: TickRate,
    pub(crate) schema: ModeSchema,
    pub(crate) roster: Roster,
    pub(crate) teams: Rc<Teams>,
    /// Where each playing team's heroes spawn.
    spawns: Vec<Position>,
    /// By unit type.
    kits: ByType<UnitKit>,
    pub(crate) structures: Vec<Structure>,
    /// `ctx.map`, as scripts read it.
    map: Map,
}

/// A structure of the map, names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Structure {
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) lane: Option<Lane>,
    pub(crate) pos: Position,
}

impl ModeBook {
    /// The book of `setup`, which passed `Mode::check`, whose script `host` compiled, its names
    /// resolved through `view` and `lanes`; an error when the teams have fewer slots than the
    /// players.
    pub(crate) fn new(
        setup: ModeSetup<'_>,
        rate: TickRate,
        host: &ScriptHost,
        view: &View,
        lanes: &Lanes,
    ) -> Result<ModeBook, ModeError> {
        let playing = setup
            .teams
            .iter()
            .map(|team| (team.name.as_str(), team.slots));
        let teams = Teams::new(playing, setup.players).ok_or(ModeError::TooManyPlayers)?;
        let mut kits = ByType::default();
        for &UnitTypeSetup { unit_type, kit } in &setup.unit_types {
            kits.set(unit_type, kit);
        }
        let mut book = ModeBook {
            rate,
            schema: ModeSchema::new(setup.script, host, setup.data),
            roster: Roster::new(setup.heroes, setup.spells),
            teams: Rc::new(teams),
            spawns: Vec::new(),
            kits,
            structures: Vec::new(),
            map: Map::new(),
        };
        book.set_map(setup.map, view, lanes);
        Ok(book)
    }

    /// Resolves the names of `map`, which the check found, unit types through `view`: its lanes,
    /// each team's spawn, its structures, and the neutral spawns `ctx.map` lists.
    fn set_map(&mut self, map: &MapData, view: &View, lanes: &Lanes) {
        let checked = "the mode's check passed";
        for team in self.teams.playing() {
            let spawn = map.spawns[team].position().expect(checked);
            self.spawns.push(spawn);
        }
        for structure in &map.structures {
            let structure = Structure {
                unit_type: view.unit_type(&structure.unit_type).expect(checked),
                team: self.teams.named(&structure.team).expect(checked),
                lane: structure
                    .lane
                    .as_ref()
                    .map(|lane| lanes.named(lane).expect(checked)),
                pos: structure.pos.position().expect(checked),
            };
            self.structures.push(structure);
        }
        let neutral_spawns = map.neutral_spawns.iter().map(|spawn| {
            let mut entry = Map::new();
            let unit_type = ImmutableString::from(spawn.unit_type.as_str());
            entry.insert("unit_type".into(), Dynamic::from(unit_type));
            let pos = spawn.pos.position().expect(checked);
            entry.insert("pos".into(), Dynamic::from(pos));
            Dynamic::from_map(entry)
        });
        let neutral_spawns: Array = neutral_spawns.collect();
        let lane_names = lanes
            .names()
            .map(|name| Dynamic::from(ImmutableString::from(name)));
        self.map
            .insert("lanes".into(), Dynamic::from_array(lane_names.collect()));
        self.map
            .insert("neutral_spawns".into(), Dynamic::from_array(neutral_spawns));
    }

    /// Where `team` walks the lanes from: the first team from each lane's start, the second from
    /// its end.
    pub(crate) fn lane_end(&self, team: Team) -> Result<PathDirection, ApiError> {
        match (team.index(), self.teams.playing().len()) {
            (0, 2..) => Ok(PathDirection::Forward),
            (1, 2..) => Ok(PathDirection::Backward),
            _ => Err(ApiError::NoLaneEnd),
        }
    }

    /// Where `team`'s heroes spawn.
    pub(crate) fn hero_spawn(&self, team: Team) -> Position {
        self.spawns[usize::from(team.index())]
    }

    pub(crate) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.kits.get(unit_type).copied()
    }

    pub(crate) fn map(&self) -> Dynamic {
        Dynamic::from_map(self.map.clone())
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
        let mut unit = world.spawn((id, pos, unit_type, team, parts));
        if let Some(combatant) = kit.combatant {
            combatant.insert(&mut unit);
        }
        if let Some(sight) = kit.sight {
            unit.insert(sight);
        }
        if let Some(step) = kit.step {
            unit.insert(step.bundle());
        }
        unit.id()
    }

    /// Spawns the hero of each player who chose one and has none yet, in slot order, at their team's
    /// spawn: under their control, with its abilities unlearned and their spells.
    pub(crate) fn spawn_heroes(&self, world: &mut World) {
        for slot in (0..self.teams.players()).map(PlayerSlot::new) {
            let pick = world.resource::<Picks>().of(slot);
            let (Some(hero), false) = (pick.hero, pick.spawned) else {
                continue;
            };
            let spells = pick.spells.clone();
            let hero = self.roster.hero_setup(hero);
            let team = self.teams.of(slot).expect("a player has a team");
            let own = hero.abilities.iter().map(|&ability| (ability, 0));
            let chosen = spells
                .iter()
                .map(|&spell| (self.roster.spell_ability(spell), 1));
            let slots = AbilitySlots::new(own.chain(chosen));
            let entity = self.spawn(
                world,
                hero.unit_type,
                team,
                self.hero_spawn(team),
                (Owner::new(slot), slots),
            );
            if let Some(resource) = hero.resource {
                world.entity_mut(entity).insert(resource);
            }
            world.resource_mut::<Picks>().of_mut(slot).spawned = true;
        }
    }

    /// Spawns `types` in order at `team`'s end of `lane`, walking it.
    pub(crate) fn spawn_wave(&self, world: &mut World, team: Team, lane: Lane, types: &[UnitType]) {
        let end = self.lane_end(team).expect("a wave's team was checked");
        let start = world
            .resource::<Lanes>()
            .waypoint(lane, 0, end)
            .expect("a lane has a waypoint");
        for &unit_type in types {
            let walker = (OnLane::new(lane), LaneWalker::start(end));
            self.spawn(world, unit_type, team, start, walker);
        }
    }
}
