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
use crate::mode::game_map::{GameMap, NeutralSpawn};
use crate::mode::map_data::MapData;
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::{ModeSetup, UnitTypeSetup};
use crate::mode::picks::Picks;
use crate::mode::roster::Roster;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::{PathDirection, PathWalker};
use crate::navigation::paths::Paths;
use crate::scripts::error::ApiError;
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
    /// Where each playing team's avatars spawn.
    spawns: Vec<Position>,
    /// By unit type.
    kits: ByType<UnitKit>,
    pub(crate) structures: Vec<Structure>,
    /// `ctx.map`, as scripts read it.
    map: GameMap,
}

/// A structure of the map, names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Structure {
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) path: Option<PathId>,
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
        let playing = setup
            .teams
            .iter()
            .map(|team| (team.name.as_str(), team.slots));
        let teams = Teams::new(playing, setup.players).ok_or(ModeError::TooManyPlayers)?;
        let mut kits = ByType::default();
        for &UnitTypeSetup { unit_type, kit, .. } in &setup.unit_types {
            kits.set(unit_type, kit);
        }
        let mut book = ModeBook {
            schema: ModeSchema::new(setup.script, host, setup.data),
            roster: Roster::new(setup.avatars, setup.loadout),
            teams: Rc::new(teams),
            bounds: setup.map.bounds,
            spawns: Vec::new(),
            kits,
            structures: Vec::new(),
            map: GameMap::default(),
        };
        book.set_map(setup.map, view, paths);
        Ok(book)
    }

    /// Resolves the names of `map`, which the check found, unit types through `view`: its paths,
    /// each team's spawn, its structures, and the neutral spawns `ctx.map` lists.
    fn set_map(&mut self, map: &MapData, view: &View, paths: &Paths) {
        let checked = "the mode's check passed";
        for team in self.teams.playing() {
            let spawn = map.spawns[team].position().expect(checked);
            self.spawns.push(spawn);
        }
        for structure in &map.structures {
            let structure = Structure {
                unit_type: view.unit_type(&structure.unit_type).expect(checked),
                team: self.teams.named(&structure.team).expect(checked),
                path: structure
                    .path
                    .as_ref()
                    .map(|path| paths.named(path).expect(checked)),
                pos: structure.pos.position().expect(checked),
            };
            self.structures.push(structure);
        }
        let neutral_spawns = map.neutral_spawns.iter().map(|spawn| NeutralSpawn {
            unit_type: ImmutableString::from(spawn.unit_type.as_str()),
            pos: spawn.pos.position().expect(checked),
        });
        self.map = GameMap::new(paths.names().map(ImmutableString::from), neutral_spawns);
    }

    /// Where `team` walks the paths from: the first team from each path's start, the second from
    /// its end.
    pub(crate) fn path_end(&self, team: Team) -> Result<PathDirection, ApiError> {
        match (team.index(), self.teams.playing().len()) {
            (0, 2..) => Ok(PathDirection::Forward),
            (1, 2..) => Ok(PathDirection::Backward),
            _ => Err(ApiError::NoPathEnd),
        }
    }

    /// Where `team`'s avatars spawn.
    pub(crate) fn avatar_spawn(&self, team: Team) -> Position {
        self.spawns[usize::from(team.index())]
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

    /// Spawns the avatar of each player who chose one and has none yet, in slot order, at their team's
    /// spawn: under their control, with its abilities unlearned and their loadout.
    pub(crate) fn spawn_avatars(&self, world: &mut World) {
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
                self.avatar_spawn(team),
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
                Stats::apply_effect(world, add, applier, |_| None);
            }
            world.resource_mut::<Picks>().of_mut(slot).spawned = true;
        }
    }

    /// Spawns `types` in order at `team`'s end of `path`, walking it.
    pub(crate) fn spawn_group(
        &self,
        world: &mut World,
        team: Team,
        path: PathId,
        types: &[UnitType],
    ) {
        let end = self
            .path_end(team)
            .expect("a spawn group's team was checked");
        let start = world
            .resource::<Paths>()
            .waypoint(path, 0, end)
            .expect("a path has a waypoint");
        for &unit_type in types {
            let walker = (OnPath::new(path), PathWalker::start(end));
            self.spawn(world, unit_type, team, start, walker);
        }
    }
}
