use std::sync::Arc;

use campfire_actions::ActionSlots;
use campfire_common::{DeclaredName, Seat};
use campfire_declare::ByType;
use campfire_script::Teams;
use campfire_units::{BodyForm, ScriptBook};
use campfire_values::{Names, Relations, Team, UnitType};

use crate::choice_specs::ChoiceSpecs;
use crate::mode_schema::ModeSchema;
use crate::mode_setup::ModeSetup;
use crate::mode_units::ModeUnits;
use crate::placed_unit::PlacedUnit;
use crate::roster::Roster;
use crate::timer_id::TimerId;

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub struct ModeSpec {
    pub(crate) schema: ModeSchema,
    pub(crate) roster: Roster,
    pub(crate) teams: Arc<Teams>,
    pub(crate) choices: ChoiceSpecs,
    /// The timers' names, by id.
    pub(crate) timers: Arc<Names<TimerId>>,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
    /// The unit types the mode spawns for a script's spawn.
    spawned: ByType<()>,
    /// The bodies of the unit types, which a box's spawn tests its room by.
    bodies: ByType<BodyForm>,
    /// The slots each unit type spawns with, which a grant to a new unit counts from.
    slots: ByType<ActionSlots>,
    /// The units the map places from the start.
    pub(crate) placed: Vec<PlacedUnit>,
    /// Whether the mode gives a `[bounty]`, whose share each player holds.
    pub(crate) bounty: bool,
    /// How the teams regard each other as the match starts, and which friendly pairs share
    /// vision.
    pub relations: Relations,
}

/// What the mode's map gives its book: the teams' names, the units it places, and how the teams
/// regard each other as the match starts.
#[derive(Debug)]
pub struct MapStart {
    pub(crate) teams: Names<Team>,
    pub(crate) placed: Vec<PlacedUnit>,
    pub relations: Relations,
}

/// What the mode's book takes of the load beside its setup: what its map gives, the ranks of
/// every loadout entry, the scripts whose hooks its script defines, and the books of its types'
/// bodies and slots, which it shares.
#[derive(Debug)]
pub struct SpecLoad<'a> {
    pub start: MapStart,
    pub units: ModeUnits,
    pub loadout_ranks: u8,
    pub scripts: &'a ScriptBook,
    pub bodies: &'a ByType<BodyForm>,
    pub slots: &'a ByType<ActionSlots>,
    /// Whether the mode gives a `[bounty]`, whose share each player holds.
    pub bounty: bool,
}

impl ModeSpec {
    /// The team of player `seat`; `None` for a seat the session does not have.
    pub fn team_of(&self, seat: Seat) -> Option<Team> {
        self.teams.of(seat)
    }

    /// The book of `setup`, for players the teams seat, with what `load` gives.
    pub fn new(setup: ModeSetup<'_>, load: SpecLoad<'_>) -> Self {
        let SpecLoad {
            start,
            units,
            loadout_ranks,
            scripts,
            bodies,
            slots,
            bounty,
        } = load;
        let MapStart {
            teams,
            placed,
            relations,
        } = start;
        let seats = setup.teams.iter().map(|team| team.seats);
        let teams = Teams::new(teams, seats, setup.players)
            .expect("the session checked its players against the teams' slots");
        let mut spawned = ByType::default();
        for &unit_type in &units.unit_types {
            spawned.set(unit_type, ());
        }
        Self {
            schema: ModeSchema::new(&setup.scripts, scripts, setup.data),
            roster: Roster::new(units.avatars, &units.loadout),
            teams: Arc::new(teams),
            choices: ChoiceSpecs::new(&setup.data.choices),
            timers: Arc::new(setup.data.timers.iter().map(DeclaredName::as_str).collect()),
            loadout_ranks,
            spawned,
            bodies: bodies.clone(),
            slots: slots.clone(),
            placed,
            bounty,
            relations,
        }
    }

    /// How many slots a unit of `unit_type` spawns with.
    pub(crate) fn spawn_slots(&self, unit_type: UnitType) -> usize {
        self.slots.get(unit_type).map_or(0, ActionSlots::len)
    }

    /// The body of a unit of `unit_type`, when it has one.
    pub(crate) fn body(&self, unit_type: UnitType) -> Option<BodyForm> {
        self.bodies.get(unit_type).copied()
    }

    /// Whether the mode spawns units of `unit_type`: a projectile or an area type it does not, as
    /// only actions deliver its units.
    pub(crate) fn spawns(&self, unit_type: UnitType) -> bool {
        self.spawned.get(unit_type).is_some()
    }
}
