use std::slice;

use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, SlotKind, SlotTimes, TimedSlots};
use campfire_combat::Respawn;
use campfire_common::{PlayerSlot, Ticks};
use campfire_declare::ByType;
use campfire_players::{HeldOut, PlayersWorld};
use campfire_script::{Effect, Frame};
use campfire_sim::{SimTick, SimWorld, StableId};
use campfire_units::{SpawnAt, Spawner, UnitsWorld};
use campfire_values::{ActionId, Rank, Relation, Team};

use crate::match_end::{MatchEnd, MatchResult};
use crate::save_asked::SaveAsked;
use crate::spawn_group::SpawnGroup;
use crate::timers::{TimerStart, Timers};

/// A change to the match that a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer(TimerStart),
    End(MatchResult),
    /// A unit the call spawns, owned by the player it names, if it names one, with the grants
    /// the call gave it, which its slots spawn with.
    SpawnUnit {
        at: SpawnAt,
        grants: Vec<SlotGrant>,
    },
    SpawnGroup(SpawnGroup),
    /// A grant to a unit that stands.
    Grant {
        id: StableId,
        grant: SlotGrant,
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

/// Actions a grant puts in one slot kind of a unit, after the slots of that kind it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotGrant {
    pub(crate) kind: SlotKind,
    /// The kind's first rank, which the actions take.
    pub(crate) rank: Option<Rank>,
    pub(crate) abilities: Vec<ActionId>,
}

impl SlotGrant {
    fn apply(&self, slots: &mut ActionSlots, held: (&mut SlotTimes, &mut ActionProgress)) {
        slots.grant(held, self.kind, &self.abilities, self.rank);
    }

    /// `slots` with each of `grants`, in order, every slot ready and nothing under way.
    fn granted(mut slots: ActionSlots, grants: &[Self]) -> TimedSlots {
        let mut times = SlotTimes::ready(&slots);
        let mut progress = ActionProgress::default();
        for grant in grants {
            grant.apply(&mut slots, (&mut times, &mut progress));
        }
        TimedSlots::new(slots)
    }
}

impl Effect for ModeEffect {
    /// Applies the effect, which a call of the mode's script queued, in the call's tick; a grant
    /// puts its abilities in its kind of a unit, after the slots of that kind it has, at the
    /// kind's first rank, and nothing for a unit that is gone. A spawned unit's grants make its
    /// slots before it spawns, so it spawns whole.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        match self {
            Self::Timer(start) => world.resource_mut::<Timers>().set(now, start),
            Self::End(result) => {
                let tick = world.resource::<SimTick>().start();
                let entity = world
                    .match_entity()
                    .expect("a match that runs has its entity");
                world.entity_mut(entity).insert(MatchEnd::new(tick, result));
            }
            Self::SpawnUnit { at, grants } => {
                let spawner = world.non_send::<Spawner>().clone();
                if grants.is_empty() {
                    spawner.spawn(world, at, ());
                } else {
                    let book = world.get_resource::<ByType<ActionSlots>>();
                    let slots = book
                        .and_then(|book| book.get(at.unit_type))
                        .cloned()
                        .unwrap_or_else(|| ActionSlots::new([]));
                    spawner.spawn(world, at, SlotGrant::granted(slots, &grants));
                }
            }
            Self::SpawnGroup(group) => group.spawn(world),
            Self::Grant { id, grant } => {
                let Some(entity) = world.entity_of(id) else {
                    return;
                };
                let mut unit = world.entity_mut(entity);
                let held = unit
                    .get_components_mut::<(&mut ActionSlots, &mut SlotTimes, &mut ActionProgress)>(
                    );
                if let Ok((mut slots, mut times, mut progress)) = held {
                    grant.apply(&mut slots, (&mut times, &mut progress));
                } else {
                    let granted = SlotGrant::granted(ActionSlots::new([]), slice::from_ref(&grant));
                    unit.insert(granted);
                }
            }
            Self::Respawn { id, ticks } => {
                let entity = world.entity_of(id);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = now.after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            Self::Learn { id, slot } => {
                let entity = world.entity_of(id);
                let entity = entity.expect("a unit the view read is in the world");
                let slots = world.get_mut::<ActionSlots>(entity);
                slots.expect("a unit with ability slots").learn(slot);
            }
            Self::SetRelation { a, b, relation } => {
                world.relations_mut().set_keeping_vision(a, b, relation);
            }
            Self::SetOut { slot, out } => {
                let player = world.player_entity(slot);
                let mut player = world.entity_mut(player.expect("every slot has its entity"));
                if out {
                    player.insert(HeldOut);
                } else {
                    player.remove::<HeldOut>();
                }
            }
            // The boundary that ends the call's tick, whatever the call counts its time from, as an
            // input's call counts from the tick's start; the match's start asks for its own.
            Self::Save => {
                let at = world.resource::<SimTick>().end();
                world.insert_resource(SaveAsked::new(at));
            }
        }
    }
}
