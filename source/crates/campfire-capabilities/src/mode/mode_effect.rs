use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_sim::StableId;

use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::combat::respawn::Respawn;
use crate::mode::group_unit::GroupUnit;
use crate::mode::match_end::MatchEnd;
use crate::mode::match_end::MatchResult;
use crate::mode::mode_book::ModeBook;
use crate::mode::save_asked::SaveAsked;
use crate::mode::timers::Timers;
use crate::navigation::path_walker::PathEnd;
use crate::scripts::ctx::Ctx;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::scripts::state_value::StateValue;
use crate::units::action_id::ActionId;
use crate::units::path_id::PathId;
use crate::units::relations::Relations;
use crate::units::spawner::SpawnAt;
use crate::units::team::Team;
use crate::values::attitude::Attitude;
use crate::values::rank::Rank;
use campfire_sim::{EntityIndex, SimTick};

/// A change to the match that a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer {
        name: String,
        ticks: Ticks,
        repeat: bool,
        data: Option<StateValue>,
    },
    End(MatchResult),
    /// Owned by the player, if the call names one.
    SpawnUnit {
        at: SpawnAt,
        owner: Option<PlayerSlot>,
    },
    SpawnGroup {
        team: Team,
        path: PathId,
        from: PathEnd,
        units: Vec<GroupUnit>,
    },
    Grant {
        unit: StableId,
        kind: SlotKind,
        /// The kind's first rank, which the actions take.
        rank: Option<Rank>,
        abilities: Vec<ActionId>,
    },
    Respawn {
        unit: StableId,
        ticks: Ticks,
    },
    Learn {
        unit: StableId,
        slot: u8,
    },
    SetRelation {
        a: Team,
        b: Team,
        attitude: Attitude,
    },
    /// A save, at the end of the tick.
    Save,
}

impl Effect for ModeEffect {
    /// Applies the effect, which a call of the mode's script queued in tick `now`; a grant puts its
    /// abilities in its kind of a unit, after the slots of that kind it has, at the kind's first
    /// rank, and nothing for a unit that is gone.
    fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
        let ctx = world.non_send::<Ctx>().clone();
        let book = ModeBook::of_match(&ctx);
        match self {
            ModeEffect::Timer {
                name,
                ticks,
                repeat,
                data,
            } => world
                .resource_mut::<Timers>()
                .set(now, name, ticks, repeat, data),
            ModeEffect::End(result) => {
                let tick = world.resource::<SimTick>().start();
                world.insert_resource(MatchEnd::new(tick, result));
            }
            ModeEffect::SpawnUnit { at, owner } => {
                book.spawn_owned(world, at, owner);
            }
            ModeEffect::SpawnGroup {
                team,
                path,
                from,
                units,
            } => book.spawn_group(world, team, path, from, &units),
            ModeEffect::Grant {
                unit,
                kind,
                rank,
                abilities,
            } => {
                let Some(entity) = world.resource::<EntityIndex>().get(unit) else {
                    return;
                };
                let mut unit = world.entity_mut(entity);
                if let Some(mut slots) = unit.get_mut::<ActionSlots>() {
                    slots.grant(kind, &abilities, rank);
                } else {
                    let slots = abilities.iter().map(|&ability| (ability, kind, rank));
                    unit.insert(ActionSlots::new(slots));
                }
            }
            ModeEffect::Respawn { unit, ticks } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = now.after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            ModeEffect::Learn { unit, slot } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a unit the view read is in the world");
                let slots = world.get_mut::<ActionSlots>(entity);
                slots.expect("a unit with ability slots").learn(slot);
            }
            ModeEffect::SetRelation { a, b, attitude } => {
                world
                    .resource_mut::<Relations>()
                    .set_attitude(a, b, attitude);
            }
            // A call's `now` is the end of its tick.
            ModeEffect::Save => world.insert_resource(SaveAsked::new(now)),
        }
    }
}
