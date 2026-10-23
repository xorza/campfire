use std::slice;

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, SlotKind, SlotsMut, SlotsMutItem, TimedSlots};
use campfire_common::PlayerSlot;
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_players::{HeldOut, PlayersWorld};
use campfire_script::{Effect, EffectKind, Frame, ListPlace};
use campfire_sim::{SimTick, SimWorld, StableId};
use campfire_units::{SpawnAt, Spawner, UnitsWorld};
use campfire_values::{ActionId, Rank, Relation, Team, UnitType};

use crate::bounty_share::BountyShare;
use crate::match_end::{MatchEnd, MatchResult};
use crate::save_asked::SaveAsked;
use crate::spawn_group::SpawnGroup;
use crate::timer_id::TimerId;
use crate::timers::{TimerStart, Timers};

/// A change to the match that a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer(TimerStart),
    /// The end of every timer of the name that `script`, a place in the mode's list, set.
    CancelTimer {
        script: ListPlace,
        name: TimerId,
    },
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
    SetRelation {
        a: Team,
        b: Team,
        relation: Relation,
    },
    /// Holds `slot` out of play while `out`.
    SetOut {
        player_slot: PlayerSlot,
        out: bool,
    },
    /// Sets `slot`'s bounty share, 0 or more, of a mode of a `[bounty]`.
    SetBountyShare {
        player_slot: PlayerSlot,
        share: Num,
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
    fn apply(&self, unit: &mut SlotsMutItem<'_, '_>) {
        unit.grant(self.kind, &self.abilities, self.rank);
    }

    fn add_to(&self, slots: ActionSlots) -> ActionSlots {
        slots.with_granted(self.kind, &self.abilities, self.rank)
    }
}

/// `slots` with each of `grants`, in order, every slot ready and nothing under way.
fn granted(slots: ActionSlots, grants: &[SlotGrant]) -> TimedSlots {
    TimedSlots::new(
        grants
            .iter()
            .fold(slots, |slots, grant| grant.add_to(slots)),
    )
}

impl Effect for ModeEffect {
    const KIND: EffectKind = EffectKind::Change;

    /// Applies the effect, which a call of the mode's script queued, in the call's tick; a grant
    /// puts its abilities in its kind of a unit, after the slots of that kind it has, at the
    /// kind's first rank, and nothing for a unit that is gone. A spawned unit's grants make its
    /// slots before it spawns, so it spawns whole.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        match self {
            Self::Timer(start) => world.resource_mut::<Timers>().set(now, start),
            Self::CancelTimer { script, name } => {
                world.resource_mut::<Timers>().cancel(script, name);
            }
            Self::End(result) => {
                let tick = world.resource::<SimTick>().start();
                let entity = world
                    .match_entity()
                    .expect("a match that runs has its entity");
                world.entity_mut(entity).insert(MatchEnd::new(tick, result));
            }
            Self::SpawnUnit { at, grants } => {
                if grants.is_empty() {
                    Spawner::spawn(world, at, ());
                } else {
                    let book = world.get_resource::<ById<UnitType, ActionSlots>>();
                    let slots = book
                        .and_then(|book| book.get(at.unit_type))
                        .cloned()
                        .unwrap_or_else(|| ActionSlots::new([]));
                    Spawner::spawn(world, at, granted(slots, &grants));
                }
            }
            Self::SpawnGroup(group) => group.spawn(world),
            Self::Grant { id, grant } => {
                let Some(entity) = world.entity_of(id) else {
                    return;
                };
                let mut unit = world.entity_mut(entity);
                if let Ok(mut held) = unit.get_components_mut::<SlotsMut>() {
                    grant.apply(&mut held);
                } else {
                    let granted = granted(ActionSlots::new([]), slice::from_ref(&grant));
                    unit.insert(granted);
                }
            }
            Self::SetRelation { a, b, relation } => {
                world.relations_mut().set_keeping_vision(a, b, relation);
            }
            Self::SetOut { player_slot, out } => {
                let player = world.player_entity(player_slot);
                let mut player = world.entity_mut(player.expect("every slot has its entity"));
                if out {
                    player.insert(HeldOut);
                } else {
                    player.remove::<HeldOut>();
                }
            }
            Self::SetBountyShare { player_slot, share } => {
                let player = world
                    .player_entity(player_slot)
                    .expect("every slot has its entity");
                world.entity_mut(player).insert(BountyShare(share));
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
