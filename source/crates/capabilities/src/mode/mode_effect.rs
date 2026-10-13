use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Ticks};
use campfire_sim::{SimTick, SimWorld, StableId};

use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_times::SlotTimes;
use crate::combat::respawn::Respawn;
use crate::mode::match_end::{MatchEnd, MatchResult};
use crate::mode::save_asked::SaveAsked;
use crate::mode::spawn_group::SpawnGroup;
use crate::mode::timers::Timers;
use crate::players::held_out::HeldOut;
use crate::players::players_world::PlayersWorld;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::scripts::state_value::StateValue;
use crate::units::action_id::ActionId;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::team::Team;
use crate::units::units_world::UnitsWorld;
use crate::units::units_world::UnitsWorldCrate;
use crate::values::rank::Rank;
use crate::values::relation::Relation;

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
    /// A unit the call spawns, owned by the player it names, if it names one.
    SpawnUnit(SpawnAt),
    SpawnGroup(SpawnGroup),
    Grant {
        id: StableId,
        kind: SlotKind,
        /// The kind's first rank, which the actions take.
        rank: Option<Rank>,
        abilities: Vec<ActionId>,
    },
    Respawn {
        id: StableId,
        ticks: Ticks,
    },
    Learn {
        id: StableId,
        slot: u8,
    },
    SetRelation {
        a: Team,
        b: Team,
        relation: Relation,
    },
    /// Holds `slot` out of play while `out`.
    SetOut {
        slot: PlayerSlot,
        out: bool,
    },
    /// A save, at the end of the tick.
    Save,
}

impl Effect for ModeEffect {
    /// Applies the effect, which a call of the mode's script queued, in the call's tick; a grant
    /// puts its abilities in its kind of a unit, after the slots of that kind it has, at the
    /// kind's first rank, and nothing for a unit that is gone.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
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
                let entity = world
                    .match_entity()
                    .expect("a match that runs has its entity");
                world.entity_mut(entity).insert(MatchEnd::new(tick, result));
            }
            ModeEffect::SpawnUnit(at) => {
                let spawner = world.non_send::<Spawner>().clone();
                spawner.spawn(world, at, ());
            }
            ModeEffect::SpawnGroup(group) => group.spawn(world),
            ModeEffect::Grant {
                id,
                kind,
                rank,
                abilities,
            } => {
                let Some(entity) = world.entity_of(id) else {
                    return;
                };
                let mut unit = world.entity_mut(entity);
                if let Ok((mut slots, mut times)) =
                    unit.get_components_mut::<(&mut ActionSlots, &mut SlotTimes)>()
                {
                    slots.grant(&mut times, kind, &abilities, rank);
                } else {
                    let slots = abilities.iter().map(|&action| (action, kind, rank));
                    unit.insert(ActionSlots::new(slots).timed());
                }
            }
            ModeEffect::Respawn { id, ticks } => {
                let entity = world.entity_of(id);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = now.after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            ModeEffect::Learn { id, slot } => {
                let entity = world.entity_of(id);
                let entity = entity.expect("a unit the view read is in the world");
                let slots = world.get_mut::<ActionSlots>(entity);
                slots.expect("a unit with ability slots").learn(slot);
            }
            ModeEffect::SetRelation { a, b, relation } => {
                world.relations_mut().set_keeping_vision(a, b, relation);
            }
            ModeEffect::SetOut { slot, out } => {
                let player = world.player_entity(slot);
                let mut player = world.entity_mut(player.expect("every slot has its entity"));
                if out {
                    player.insert(HeldOut);
                } else {
                    player.remove::<HeldOut>();
                }
            }
            // A call's `now` is the end of its tick.
            ModeEffect::Save => world.insert_resource(SaveAsked::new(now)),
        }
    }
}
