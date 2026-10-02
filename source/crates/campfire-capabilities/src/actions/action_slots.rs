use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_target::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::units::action_id::ActionId;

/// A unit's actions: its slots, kind after kind in the mode's order, each an action at a rank
/// with its cooldown; the action it was ordered or has under way, one at a time; and the unit its
/// attacks aim at, which it attacks again each time a weapon is ready, until another order.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlots {
    slots: Vec<ActionSlot>,
    underway: Option<InProgress>,
    attack_target: Option<StableId>,
}

/// One slot: the action, its kind, its rank, 0 while not learned, and the first tick it may start
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlot {
    pub action: ActionId,
    pub kind: SlotKind,
    pub rank: u8,
    pub ready_at: Tick,
}

/// What a unit has under way: an attack in its windup, or an action it was ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum InProgress {
    /// The weapon in `slot`, started at `target` from the attack target, to resolve in
    /// `resolves_at`: an attack is under way only once started.
    Attack {
        slot: u8,
        target: StableId,
        resolves_at: Tick,
    },
    /// The action `aim` names, as ordered: not checked yet while `resolves_at` is `None`, then
    /// started, to resolve in that tick. Whether it casts or trains is its action's kind, and only
    /// a cast starts.
    Order {
        aim: SlotAim,
        resolves_at: Option<Tick>,
    },
}

/// The action in `slot`, and what it is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SlotAim {
    pub(crate) slot: u8,
    pub(crate) target: ActionTarget,
}

impl InProgress {
    /// The slot whose action is under way.
    pub(crate) const fn slot(self) -> u8 {
        match self {
            InProgress::Attack { slot, .. }
            | InProgress::Order {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The slot, to move when slots are added before it.
    const fn slot_mut(&mut self) -> &mut u8 {
        match self {
            InProgress::Attack { slot, .. }
            | InProgress::Order {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The aim of a started order that resolves by `now`: of a cast, as only a cast starts.
    pub(crate) fn cast_due(self, now: Tick) -> Option<SlotAim> {
        match self {
            InProgress::Order {
                aim,
                resolves_at: Some(at),
            } if at <= now => Some(aim),
            _ => None,
        }
    }

    /// The tick it resolves in, once started.
    pub(crate) const fn resolves_at(self) -> Option<Tick> {
        match self {
            InProgress::Attack { resolves_at, .. } => Some(resolves_at),
            InProgress::Order { resolves_at, .. } => resolves_at,
        }
    }
}

impl ActionSlots {
    /// The most slots a unit holds: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    /// Slots of `(action, kind, rank)`, in the order of their kinds, each ready at once.
    pub fn new(slots: impl IntoIterator<Item = (ActionId, SlotKind, u8)>) -> ActionSlots {
        let slots: Vec<ActionSlot> = slots
            .into_iter()
            .map(|(action, kind, rank)| ActionSlot {
                action,
                kind,
                rank,
                ready_at: Tick::ZERO,
            })
            .collect();
        debug_assert!(slots.is_sorted_by_key(|slot| slot.kind));
        ActionSlots {
            slots,
            underway: None,
            attack_target: None,
        }
    }

    /// Puts `actions` in `kind` at `rank`, each ready at once, after the slots of that kind it
    /// has: the slots of later kinds, and an action under way from one, move along.
    pub(crate) fn grant(&mut self, kind: SlotKind, actions: &[ActionId], rank: u8) {
        let at = self.slots.partition_point(|slot| slot.kind <= kind);
        let added = actions.iter().map(|&action| ActionSlot {
            action,
            kind,
            rank,
            ready_at: Tick::ZERO,
        });
        self.slots.splice(at..at, added);
        if let Some(underway) = &mut self.underway
            && usize::from(underway.slot()) >= at
        {
            let moved = usize::from(underway.slot()) + actions.len();
            *underway.slot_mut() = u8::try_from(moved).expect("a unit's slots fit u8");
        }
    }

    pub fn slot(&self, slot: u8) -> Option<ActionSlot> {
        self.slots.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = ActionSlot> + '_ {
        self.slots.iter().copied()
    }

    /// Raises the action in `slot` a rank; the caller checked it has one more.
    pub(crate) fn learn(&mut self, slot: u8) {
        let slot = &mut self.slots[usize::from(slot)];
        slot.rank = slot
            .rank
            .checked_add(1)
            .expect("a rank below the action's ranks");
    }

    /// Orders the action in `slot` at `target`, in place of any other action not resolved yet.
    pub(crate) const fn order(&mut self, slot: u8, target: ActionTarget) {
        self.underway = Some(InProgress::Order {
            aim: SlotAim { slot, target },
            resolves_at: None,
        });
    }

    pub(crate) const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept.
    pub(crate) const fn start(&mut self, resolves_at: Tick, target: ActionTarget) {
        if let Some(InProgress::Order {
            aim,
            resolves_at: started,
        }) = &mut self.underway
        {
            *started = Some(resolves_at);
            aim.target = target;
        }
    }

    /// Starts an attack with the weapon in `slot` at the attack target, to resolve in
    /// `resolves_at`.
    pub(crate) const fn start_attack(&mut self, slot: u8, resolves_at: Tick) {
        let target = self.attack_target.expect("an attack starts at its target");
        self.underway = Some(InProgress::Attack {
            slot,
            target,
            resolves_at,
        });
    }

    /// Stops what is under way and spends nothing: a cast goes back to its order, which starts it
    /// again from its check; an attack starts again from the attack target when it may.
    pub(crate) const fn interrupt(&mut self) {
        match &mut self.underway {
            Some(InProgress::Attack { .. }) => self.underway = None,
            Some(InProgress::Order { resolves_at, .. }) => *resolves_at = None,
            None => {}
        }
    }

    /// Ends what is under way, resolved or not.
    pub(crate) const fn stop(&mut self) {
        self.underway = None;
    }

    /// The unit its attacks aim at.
    pub const fn attack_target(&self) -> Option<StableId> {
        self.attack_target
    }

    /// The target of the attack in its windup, if one is.
    pub const fn attacking(&self) -> Option<StableId> {
        match self.underway {
            Some(InProgress::Attack { target, .. }) => Some(target),
            _ => None,
        }
    }

    /// Attacks `target` from now on; another target than the current one cancels an attack in
    /// its windup and spends nothing.
    pub(crate) fn set_attack_target(&mut self, target: Option<StableId>) {
        if target != self.attack_target && self.attacking().is_some() {
            self.underway = None;
        }
        self.attack_target = target;
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        self.slots[usize::from(slot)].ready_at = ready_at;
    }
}

impl SimComponent for ActionSlots {
    const NAME: &'static str = "actions.slots";

    // An action the book lacks, a rank past its ranks, or what is under way of a slot it does
    // not have, an attack of no weapon or an order of one, has no rules for a cast to follow.
    fn check(&self, world: &World, _: Entity) -> bool {
        let Some(book) = world.get_resource::<ActionBook>() else {
            return self.slots.is_empty() && self.underway.is_none();
        };
        let held = |slot: &ActionSlot| {
            book.get(slot.action)
                .is_some_and(|action| action.slots_at(slot.rank))
        };
        // An attack under way started its windup before it resolves, no sooner than tick 0.
        let underway = self.underway.is_none_or(|underway| {
            let slot = self.slots.get(usize::from(underway.slot()));
            let action = slot.and_then(|slot| Some((slot.rank, book.get(slot.action)?)));
            let attacks = matches!(underway, InProgress::Attack { .. });
            action.is_some_and(|(rank, action)| {
                let windup = action.values(rank).windup;
                let started = !attacks
                    || underway
                        .resolves_at()
                        .is_some_and(|at| at.get() >= windup.get());
                (action.kind.kind() == ActionKind::Attack) == attacks && started
            })
        });
        let times = self.slots.iter().all(|slot| slot.ready_at <= Tick::LIMIT)
            && self
                .underway
                .and_then(InProgress::resolves_at)
                .is_none_or(|at| at <= Tick::LIMIT);
        self.slots.len() <= ActionSlots::LIMIT && self.slots.iter().all(held) && underway && times
    }
}
