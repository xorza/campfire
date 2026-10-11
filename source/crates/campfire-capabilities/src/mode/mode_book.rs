use std::ops::Range;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::{EntityRef, World};
use campfire_common::PlayerSlot;
use campfire_math::Num;
use campfire_sim::Position;

use crate::actions::action_slots::ActionSlots;
use crate::mode::choice_book::ChoiceBook;
use crate::mode::game_map::GameMap;
use crate::mode::group_unit::GroupUnit;
use crate::mode::mode_schema::ModeSchema;
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::placed_unit::PlacedUnit;
use crate::mode::roster::Roster;
use crate::mode::slot_action::SlotAction;
use crate::mode::unit_kit::UnitKit;
use crate::mode::unit_kit::kit_part::{KitPart, KitSpawn};
use crate::navigation::Navigation;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::{PathEnd, PathWalker};
use crate::navigation::paths::Paths;
use crate::progression::track_book::TrackBook;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_book::ScriptBook;
use crate::stats::applier::Applier;
use crate::stats::lifetime::Hold;
use crate::stats::stats_effect::StatsEffect;
use crate::stats::unit_stats::UnitStats;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::facing::Facing;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::relations::Relations;
use crate::units::spawn_at::SpawnAt;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::unit_state::{SyncAll, SyncNone};
use crate::units::unit_type::UnitType;
use crate::values::rank::Rank;

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
    /// How the teams regard each other as the match starts, and the mode's state, each class.
    pub(crate) relations: Relations,
    pub(crate) state_none: ModeState<SyncNone>,
    pub(crate) state_all: ModeState<SyncAll>,
    /// `ctx.map`, as scripts read it, and where avatars spawn.
    map: GameMap,
    /// The parts a unit's kit may give it that the match's state holds.
    kit_parts: Vec<KitPart>,
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

    /// The mode of the match `ctx` runs in, which has one: the mode's own systems and calls run
    /// only in such a match.
    pub(crate) fn of_match(ctx: &Ctx) -> &ModeBook {
        ModeBook::of(ctx).expect("a match with a mode")
    }

    /// The mode of the match `ctx` runs in; an error in a match with none.
    pub(crate) fn of_or_fail(ctx: &Ctx) -> Checked<&ModeBook> {
        ModeBook::of(ctx).ok_or_else(|| ApiError::NoMode.fail().into())
    }

    /// The book of `setup`, whose script defines the hooks `scripts` gives, for players the teams
    /// seat, with the units its map places, the `relations` its teams start with, `map` as
    /// scripts read it, `loadout_ranks`, the ranks of every loadout entry, and `kit_parts`, the
    /// parts a kit may give that the match's state holds.
    pub(crate) fn new(
        setup: ModeSetup<'_>,
        scripts: &ScriptBook,
        placed: Vec<PlacedUnit>,
        relations: Relations,
        map: GameMap,
        loadout_ranks: u8,
        kit_parts: Vec<KitPart>,
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
            loadout_ranks,
            types,
            actions,
            placed,
            relations,
            state_none: ModeState::initial(setup.data),
            state_all: ModeState::initial(setup.data),
            map,
            kit_parts,
        }
    }

    /// The actions of `unit_type`, kind after kind.
    pub(crate) fn actions(&self, unit_type: UnitType) -> &[SlotAction] {
        self.types
            .get(unit_type)
            .map_or(&[], |held| &self.actions[held.actions.clone()])
    }

    /// The slots a unit of `unit_type` spawns with: its actions, each at the first rank of its
    /// kind, then an empty slot of its inventory's kind for each inventory slot; none for a type
    /// with neither.
    pub(crate) fn spawn_slots(&self, unit_type: UnitType) -> Option<ActionSlots> {
        let inventory = self.kit(unit_type)?.inventory;
        if !self.has_slots(unit_type) {
            return None;
        }
        let actions = self
            .actions(unit_type)
            .iter()
            .map(|action| (action.ability, action.kind, action.rank));
        let slots = ActionSlots::new(actions);
        Some(match inventory {
            Some(inventory) => slots.with_empty(inventory.kind, inventory.slots.get()),
            None => slots,
        })
    }

    pub(super) fn kit(&self, unit_type: UnitType) -> Option<UnitKit> {
        self.types.get(unit_type).map(|held| held.kit)
    }

    /// Whether a unit of `unit_type` spawns with slots: actions, or an inventory.
    fn has_slots(&self, unit_type: UnitType) -> bool {
        let inventory = self
            .kit(unit_type)
            .is_some_and(|kit| kit.inventory.is_some());
        inventory || !self.actions(unit_type).is_empty()
    }

    /// Whether `unit` holds every part a unit of its type spawns with, of a match whose `level`
    /// track is `level_track`: its slots when its type gives any, and each part of `kit_parts`
    /// its kit gives.
    pub(crate) fn holds_kit(&self, unit: EntityRef<'_>, level_track: Option<TrackId>) -> bool {
        let Some((&unit_type, kit)) = unit
            .get::<UnitType>()
            .and_then(|unit_type| Some((unit_type, self.kit(*unit_type)?)))
        else {
            return false;
        };
        let Some(&pos) = unit.get::<Position>() else {
            return false;
        };
        let at = KitSpawn {
            kit: &kit,
            level_track,
            pos,
            angle: unit
                .get::<Facing>()
                .map_or(Num::ZERO, |facing| facing.degrees()),
        };
        let slots = !self.has_slots(unit_type) || unit.contains::<ActionSlots>();
        slots && self.kit_parts.iter().all(|part| part.held_by(unit, &at))
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
        self.spawn(world, at, owner.map(Owner::new), ())
    }

    /// Spawns `at`, with its kit, its script state at its type's defaults, its actions, each at
    /// the first rank of its kind, an empty slot of its inventory's kind for each inventory slot,
    /// its passive, and `parts`.
    pub(crate) fn spawn(
        &self,
        world: &mut World,
        at: SpawnAt,
        owner: Option<Owner>,
        parts: impl Bundle,
    ) -> Entity {
        let SpawnAt {
            id,
            unit_type,
            pos,
            angle,
            ..
        } = at;
        let kit = self
            .kit(unit_type)
            .expect("a unit type of the mode has a kit");
        let level_track = world
            .get_resource::<TrackBook>()
            .and_then(TrackBook::level_track);
        let mut unit = at.core(owner).spawn(world, (UnitStats::default(), parts));
        let at = KitSpawn {
            kit: &kit,
            level_track,
            pos,
            angle,
        };
        KitPart::give_every(&at, &mut unit);
        if let Some(slots) = self.spawn_slots(unit_type) {
            unit.insert(slots.timed());
        }
        let boxed = unit
            .get::<Body>()
            .is_some_and(|body| body.half_edges().is_some());
        let entity = unit.id();
        if let Some(passive) = self.types.get(unit_type).and_then(|held| held.passive) {
            let applier = Applier {
                source: Some(id),
                ability: None,
                rank: Rank::FIRST,
                hold: Some(Hold::Passive),
            };
            let add = StatsEffect::Add {
                target: id,
                id: passive,
                duration: None,
            };
            add.apply_by(world, applier);
        }
        if boxed {
            Navigation::make_room(world, entity);
        }
        entity
    }

    /// Spawns `units` of `team`, each a unit type with the id a call took for it, in order at the
    /// end `from` of `path`, walking it from there as one spawn group.
    pub(crate) fn spawn_group(
        &self,
        world: &mut World,
        team: Team,
        path: PathId,
        from: PathEnd,
        units: &[GroupUnit],
    ) {
        let Some(first) = units.first() else {
            return;
        };
        let pos = world
            .resource::<Paths>()
            .waypoint(path, 0, from)
            .expect("a path has a waypoint");
        for &GroupUnit { unit_type, id } in units {
            let walker = (OnPath::new(path), PathWalker::start(from, first.id));
            let at = SpawnAt {
                id,
                unit_type,
                team,
                pos,
                angle: Num::ZERO,
            };
            self.spawn(world, at, None, walker);
        }
    }
}
