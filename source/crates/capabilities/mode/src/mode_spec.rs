use std::rc::Rc;

use campfire_actions::ActionSlots;
use campfire_declare::ByType;
use campfire_script::{ApiError, Checked, Ctx, Teams};
use campfire_units::{BodyForm, ScriptBook};
use campfire_values::{Names, Relations, Team, UnitType};

use crate::choice_specs::ChoiceSpecs;
use crate::game_map::GameMap;
use crate::mode_schema::ModeSchema;
use crate::mode_setup::ModeSetup;
use crate::placed_unit::PlacedUnit;
use crate::roster::Roster;
use crate::unit_type_setup::UnitTypeSetup;

/// The mode's package data as a match runs it, names resolved: package data, not state. A restore
/// loads it from the packages, as a new match does.
#[derive(Debug)]
pub struct ModeSpec {
    pub(crate) schema: ModeSchema,
    pub(crate) roster: Roster,
    pub(crate) teams: Rc<Teams>,
    pub(crate) choices: ChoiceSpecs,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
    /// What the mode keeps of each of its unit types.
    types: ByType<ModeType>,
    /// The units the map places from the start.
    pub(crate) placed: Vec<PlacedUnit>,
    /// How the teams regard each other as the match starts, and the mode's state, each class.
    pub relations: Relations,
    /// `ctx.map`, as scripts read it, and where avatars spawn.
    map: GameMap,
}

/// What the mode keeps of a unit type it spawns for a script's spawn, which checks them before
/// the unit is: its body and how many slots it spawns with. The books hold its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeType {
    body: Option<BodyForm>,
    slots: usize,
}

/// What the mode's map gives its book: the teams' names, the units it places, how the teams
/// regard each other as the match starts, and the map as scripts read it.
#[derive(Debug)]
pub struct MapStart {
    pub(crate) teams: Names<Team>,
    pub(crate) placed: Vec<PlacedUnit>,
    pub relations: Relations,
    pub(crate) map: GameMap,
}

impl ModeSpec {
    /// The mode of the match `ctx` runs in, once it installed.
    pub fn of(ctx: &Ctx) -> Option<&ModeSpec> {
        ctx.mode()?.downcast_ref()
    }

    /// The mode of the match `ctx` runs in, which has one: the mode's own systems and calls run
    /// only in such a match.
    pub(crate) fn of_match(ctx: &Ctx) -> &ModeSpec {
        ModeSpec::of(ctx).expect("a match with a mode")
    }

    /// The mode of the match `ctx` runs in; an error in a match with none.
    pub(crate) fn of_or_fail(ctx: &Ctx) -> Checked<&ModeSpec> {
        ModeSpec::of(ctx).ok_or_else(|| ApiError::NoMode.fail().into())
    }

    /// The book of `setup`, whose script defines the hooks `scripts` gives, for players the teams
    /// seat, with what its map gives, `start`, `loadout_ranks`, the ranks of every loadout entry,
    /// and the `bodies` and the `spawn_slots` of its types.
    pub fn new(
        setup: ModeSetup<'_>,
        scripts: &ScriptBook,
        start: MapStart,
        loadout_ranks: u8,
        bodies: &ByType<BodyForm>,
        spawn_slots: &ByType<ActionSlots>,
    ) -> ModeSpec {
        let MapStart {
            teams,
            placed,
            relations,
            map,
        } = start;
        let slots = setup.teams.iter().map(|team| team.slots);
        let teams = Teams::new(teams, slots, setup.players)
            .expect("the session checked its players against the teams' slots");
        let mut types = ByType::default();
        for &UnitTypeSetup { unit_type, .. } in &setup.units.unit_types {
            let held = ModeType {
                body: bodies.get(unit_type).copied(),
                slots: spawn_slots.get(unit_type).map_or(0, ActionSlots::len),
            };
            types.set(unit_type, held);
        }
        ModeSpec {
            schema: ModeSchema::new(setup.script, scripts, setup.data),
            roster: Roster::new(setup.units.avatars, &setup.units.loadout),
            teams: Rc::new(teams),
            choices: ChoiceSpecs::new(&setup.data.choices),
            loadout_ranks,
            types,
            placed,
            relations,
            map,
        }
    }

    /// How many slots a unit of `unit_type` spawns with.
    pub(crate) fn spawn_slots(&self, unit_type: UnitType) -> usize {
        self.types.get(unit_type).map_or(0, |held| held.slots)
    }

    /// The body of a unit of `unit_type`, when the mode spawns the type and it has one.
    pub(crate) fn body(&self, unit_type: UnitType) -> Option<BodyForm> {
        self.types.get(unit_type)?.body
    }

    /// Whether the mode spawns units of `unit_type`: a projectile or an area type it does not, as
    /// only actions deliver its units.
    pub(crate) fn spawns(&self, unit_type: UnitType) -> bool {
        self.types.get(unit_type).is_some()
    }

    pub(crate) fn map(&self) -> GameMap {
        self.map.clone()
    }
}
