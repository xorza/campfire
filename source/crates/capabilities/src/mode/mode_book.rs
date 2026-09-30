use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::rhai::{Array, Dynamic, ImmutableString, Map};
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{IdAllocator, Position, StableId, TickRate};

use crate::abilities::ability_book::AbilityId;
use crate::abilities::ability_slots::AbilitySlots;
use crate::combat::team::Team;
use crate::control::controller::Controller;
use crate::mode::error::ModeError;
use crate::mode::manifest::TeamManifest;
use crate::mode::map_data::MapData;
use crate::mode::mode_data::{InputType, ListEntry, ModeParam};
use crate::mode::mode_setup::{HeroSetup, ModeSetup, SpellSetup, UnitTypeSetup};
use crate::mode::picks::Picks;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::lane_walker::{LaneWalker, PathDirection};
use crate::navigation::lanes::Lanes;
use crate::navigation::on_lane::OnLane;
use crate::units::error::ApiError;
use crate::units::hook::Hook;
use crate::units::script_view::View;
use crate::units::state_decl::StateType;
use crate::units::state_value::StateValue;
use crate::units::unit_type::UnitType;

/// The name of the team neutral units spawn on: one more team, an enemy of every other.
pub(crate) const NEUTRAL: &str = "neutral";

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct ModeBook {
    pub(crate) script: ScriptId,
    pub(crate) rate: TickRate,
    /// Which of the mode's hooks the engine calls its script defines.
    pub(crate) on_match_start: bool,
    pub(crate) on_mode_input: bool,
    pub(crate) on_timer: bool,
    /// Sorted by name.
    params: Vec<(Box<str>, ModeParam)>,
    /// Sorted by name: each state field's name and type.
    state: Vec<(Box<str>, StateType)>,
    /// Each field's first value, in the order of `state`.
    pub(crate) state_initial: Vec<StateValue>,
    /// Sorted by name.
    inputs: Vec<(Box<str>, InputType)>,
    /// The playing teams; the neutral team's index follows theirs.
    pub(crate) teams: Vec<Box<str>>,
    /// Each player's team, by slot.
    slot_teams: Vec<Team>,
    /// Where each playing team's heroes spawn.
    spawns: Vec<Position>,
    /// By unit type.
    kits: Vec<Option<UnitKit>>,
    pub(crate) heroes: Vec<HeroSetup>,
    /// Sorted by id.
    spells: Vec<SpellSetup>,
    pub(crate) structures: Vec<Structure>,
    /// `ctx.map`, as scripts read it.
    map: Map,
    pub(crate) lanes: Lanes,
}

/// A field of the mode's state: its place in `ModeState`, and its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateField {
    pub(crate) index: usize,
    pub(crate) kind: StateType,
}

/// A structure of the map, names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Structure {
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) lane: Option<u32>,
    pub(crate) pos: Position,
}

impl ModeBook {
    /// The book of `setup`, which passed `Mode::check`, its script compiled in `host`; an error
    /// when the script does not compile, or the teams have fewer slots than the players.
    pub(crate) fn new(
        setup: ModeSetup<'_>,
        rate: TickRate,
        host: &mut ScriptHost,
        view: &View,
    ) -> Result<ModeBook, ModeError> {
        let script = host.compile(setup.script).map_err(ModeError::Script)?;
        let defines = |hook: Hook| host.defines(script, hook.name(), hook.params());
        let mut kits = Vec::new();
        for &UnitTypeSetup { unit_type, kit } in &setup.unit_types {
            if kits.len() <= unit_type.index() {
                kits.resize(unit_type.index() + 1, None);
            }
            kits[unit_type.index()] = Some(kit);
        }
        let mut spells = setup.spells;
        spells.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        let mut book = ModeBook {
            script,
            rate,
            on_match_start: defines(Hook::OnMatchStart),
            on_mode_input: defines(Hook::OnModeInput),
            on_timer: defines(Hook::OnTimer),
            params: setup
                .data
                .params
                .iter()
                .map(|(name, param)| (name.as_str().into(), param.clone()))
                .collect(),
            state: Vec::new(),
            state_initial: Vec::new(),
            inputs: setup
                .data
                .inputs
                .iter()
                .map(|(name, &kind)| (name.as_str().into(), kind))
                .collect(),
            teams: Vec::new(),
            slot_teams: Vec::new(),
            spawns: Vec::new(),
            kits,
            heroes: setup.heroes,
            spells,
            structures: Vec::new(),
            map: Map::new(),
            lanes: Lanes::default(),
        };
        for (name, decl) in &setup.data.state {
            book.state.push((name.as_str().into(), decl.kind));
            book.state_initial.push(decl.initial.clone());
        }
        book.set_teams(setup.teams, setup.players)?;
        book.set_map(setup.map, view);
        Ok(book)
    }

    /// Names the playing teams, and gives each of `players` players the team of its slot: the
    /// teams' slots in their order.
    fn set_teams(&mut self, teams: &[TeamManifest], players: u32) -> Result<(), ModeError> {
        for (index, team) in teams.iter().enumerate() {
            self.teams.push(team.name.as_str().into());
            let index = u8::try_from(index).expect("the check counted the teams");
            self.slot_teams
                .extend((0..team.slots).map(|_| Team::new(index)));
        }
        if self.slot_teams.len() < players as usize {
            return Err(ModeError::TooManyPlayers);
        }
        self.slot_teams.truncate(players as usize);
        Ok(())
    }

    /// Resolves the names of `map`, which the check found, unit types through `view`: its lanes,
    /// each team's spawn, its structures, and the neutral spawns `ctx.map` lists.
    fn set_map(&mut self, map: &MapData, view: &View) {
        let checked = "the mode's check passed";
        let lanes: Vec<_> = map
            .lanes
            .iter()
            .map(|lane| {
                let points = lane
                    .points
                    .iter()
                    .map(|point| point.position().expect(checked));
                (lane.name.as_str(), points.collect::<Vec<_>>())
            })
            .collect();
        self.lanes = Lanes::new(
            lanes
                .iter()
                .map(|(name, points)| (*name, points.as_slice())),
        );
        for team in &self.teams {
            let spawn = map.spawns[&**team].position().expect(checked);
            self.spawns.push(spawn);
        }
        for structure in &map.structures {
            let structure = Structure {
                unit_type: view.unit_type(&structure.unit_type).expect(checked),
                team: self.team(&structure.team).expect(checked),
                lane: structure
                    .lane
                    .as_ref()
                    .map(|lane| self.lanes.named(lane).expect(checked)),
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
        let lane_names = self
            .lanes
            .names()
            .map(|name| Dynamic::from(ImmutableString::from(name)));
        self.map
            .insert("lanes".into(), Dynamic::from_array(lane_names.collect()));
        self.map
            .insert("neutral_spawns".into(), Dynamic::from_array(neutral_spawns));
    }

    /// The team named `name`: a playing team, or the neutral one.
    pub(crate) fn team(&self, name: &str) -> Option<Team> {
        let index = match self.teams.iter().position(|team| **team == *name) {
            Some(index) => index,
            None if name == NEUTRAL => self.teams.len(),
            None => return None,
        };
        Some(Team::new(u8::try_from(index).expect("teams fit u8")))
    }

    /// Every team's name, the neutral one last, by index.
    pub(crate) fn team_names(&self) -> Vec<Box<str>> {
        let mut names = self.teams.clone();
        names.push(NEUTRAL.into());
        names
    }

    /// Where `team` walks the lanes from: the first team from each lane's start, the second from
    /// its end.
    pub(crate) fn lane_end(&self, team: Team) -> Result<PathDirection, ApiError> {
        match (team.index(), self.teams.len()) {
            (0, 2..) => Ok(PathDirection::Forward),
            (1, 2..) => Ok(PathDirection::Backward),
            _ => Err(ApiError::NoLaneEnd),
        }
    }

    /// The one enemy team of `team`, in a mode of two teams.
    pub(crate) fn enemy_team(&self, team: Team) -> Result<Team, ApiError> {
        match (team.index(), self.teams.len()) {
            (0, 2) => Ok(Team::new(1)),
            (1, 2) => Ok(Team::new(0)),
            _ => Err(ApiError::NoEnemyTeam),
        }
    }

    /// The team of player `slot`.
    pub(crate) fn slot_team(&self, slot: u32) -> Option<Team> {
        self.slot_teams.get(slot as usize).copied()
    }

    pub(crate) fn players(&self) -> u32 {
        u32::try_from(self.slot_teams.len()).expect("players fit u32")
    }

    /// Where `team`'s heroes spawn.
    pub(crate) fn hero_spawn(&self, team: Team) -> Position {
        self.spawns[usize::from(team.index())]
    }

    pub(crate) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.kits.get(unit_type.index()).copied().flatten()
    }

    pub(crate) fn lane(&self, name: &str) -> Option<u32> {
        self.lanes.named(name)
    }

    /// The hero `id`, by its place among the mode's.
    pub(crate) fn hero(&self, id: &str) -> Option<u16> {
        let at = self.heroes.iter().position(|hero| hero.id == id)?;
        Some(u16::try_from(at).expect("heroes fit u16"))
    }

    /// The spell `id`, by its place among the mode's.
    pub(crate) fn spell(&self, id: &str) -> Option<u16> {
        let at = self
            .spells
            .binary_search_by(|spell| spell.id.as_str().cmp(id))
            .ok()?;
        Some(u16::try_from(at).expect("spells fit u16"))
    }

    pub(crate) fn spell_ability(&self, spell: u16) -> AbilityId {
        self.spells[usize::from(spell)].ability
    }

    /// The param `name`, as `ctx.p` reads it.
    pub(crate) fn param(&self, name: &str) -> Option<Dynamic> {
        let at = self
            .params
            .binary_search_by(|(held, _)| (**held).cmp(name))
            .ok()?;
        Some(match &self.params[at].1 {
            ModeParam::Value(value) => value.to_dynamic(),
            ModeParam::List(entries) => Dynamic::from_array(
                entries
                    .iter()
                    .map(|entry| match entry {
                        ListEntry::Value(value) => value.to_dynamic(),
                        ListEntry::Text(text) => {
                            Dynamic::from(ImmutableString::from(text.as_str()))
                        }
                    })
                    .collect(),
            ),
        })
    }

    /// The state field `name`.
    pub(crate) fn state_field(&self, name: &str) -> Option<StateField> {
        let at = self
            .state
            .binary_search_by(|(held, _)| (**held).cmp(name))
            .ok()?;
        Some(StateField {
            index: at,
            kind: self.state[at].1,
        })
    }

    pub(crate) fn input_type(&self, name: &str) -> Option<InputType> {
        let at = self
            .inputs
            .binary_search_by(|(held, _)| (**held).cmp(name))
            .ok()?;
        Some(self.inputs[at].1)
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
        let mut unit = world.spawn((id, pos, unit_type, parts));
        if let Some(combatant) = kit.combatant {
            combatant.insert(&mut unit, team);
        }
        if let Some(step) = kit.step {
            unit.insert(step.bundle());
        }
        unit.id()
    }

    /// Spawns the hero of each player who chose one and has none yet, in slot order, at their team's
    /// spawn: under their control, with its abilities unlearned and their spells.
    pub(crate) fn spawn_heroes(&self, world: &mut World) {
        for slot in 0..self.players() {
            let pick = &world.resource::<Picks>().0[slot as usize];
            let (Some(hero), false) = (pick.hero, pick.spawned) else {
                continue;
            };
            let spells = pick.spells.clone();
            let hero = &self.heroes[usize::from(hero)];
            let team = self.slot_team(slot).expect("a player has a team");
            let own = hero.abilities.iter().map(|&ability| (ability, 0));
            let chosen = spells.iter().map(|&spell| (self.spell_ability(spell), 1));
            let slots = AbilitySlots::new(own.chain(chosen));
            let entity = self.spawn(
                world,
                hero.unit_type,
                team,
                self.hero_spawn(team),
                (Controller::new(slot), slots),
            );
            if let Some(resource) = hero.resource {
                world.entity_mut(entity).insert(resource);
            }
            world.resource_mut::<Picks>().0[slot as usize].spawned = true;
        }
    }

    /// Spawns `types` in order at `team`'s end of `lane`, walking it.
    pub(crate) fn spawn_wave(&self, world: &mut World, team: Team, lane: u32, types: &[UnitType]) {
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
