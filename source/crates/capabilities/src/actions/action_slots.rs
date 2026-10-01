use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, Position, SimComponent, StableId, Tick};
use serde::{Deserialize, Serialize};

use crate::actions::action_book::ActionId;
use crate::actions::action_kind::ActionKind;
use crate::actions::slot_kind::SlotKind;

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

/// The action of `kind` in `slot` at `target`: ordered and not checked yet while `resolves_at`
/// is `None`, then started, to resolve in that tick. An attack is under way only once started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InProgress {
    pub(crate) slot: u8,
    pub(crate) kind: ActionKind,
    pub(crate) target: ActionTarget,
    pub(crate) resolves_at: Option<Tick>,
}

/// What an action is aimed at: nothing, a unit, or a point, which an action that aims at a
/// direction aims through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionTarget {
    None,
    Unit(StableId),
    Point(Position),
}

impl ActionTarget {
    /// The point it names in `world`: its point, or where its unit stands; `None` for no
    /// target, or a unit that is gone.
    pub(crate) fn point(self, world: &World) -> Option<Position> {
        match self {
            ActionTarget::None => None,
            ActionTarget::Point(at) => Some(at),
            ActionTarget::Unit(unit) => {
                let entity = world.resource::<EntityIndex>().get(unit)?;
                world.get::<Position>(entity).copied()
            }
        }
    }

    pub(crate) const fn unit(self) -> Option<StableId> {
        match self {
            ActionTarget::Unit(unit) => Some(unit),
            ActionTarget::None | ActionTarget::Point(_) => None,
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
            && usize::from(underway.slot) >= at
        {
            let moved = usize::from(underway.slot) + actions.len();
            underway.slot = u8::try_from(moved).expect("a unit's slots fit u8");
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

    /// Orders the action in `slot`, of `kind`, in place of any other action not resolved yet.
    pub(crate) const fn order(&mut self, slot: u8, kind: ActionKind, target: ActionTarget) {
        self.underway = Some(InProgress {
            slot,
            kind,
            target,
            resolves_at: None,
        });
    }

    pub(crate) const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept.
    pub(crate) const fn start(&mut self, resolves_at: Tick, target: ActionTarget) {
        if let Some(underway) = &mut self.underway {
            underway.resolves_at = Some(resolves_at);
            underway.target = target;
        }
    }

    /// Starts an attack with the weapon in `slot` at the attack target, to resolve in
    /// `resolves_at`.
    pub(crate) const fn start_attack(&mut self, slot: u8, resolves_at: Tick) {
        let target = self.attack_target.expect("an attack starts at its target");
        self.underway = Some(InProgress {
            slot,
            kind: ActionKind::Attack,
            target: ActionTarget::Unit(target),
            resolves_at: Some(resolves_at),
        });
    }

    /// Stops what is under way and spends nothing: a cast goes back to its order, which starts it
    /// again from its check; an attack starts again from the attack target when it may.
    pub(crate) const fn interrupt(&mut self) {
        match &mut self.underway {
            Some(InProgress {
                kind: ActionKind::Attack,
                ..
            }) => self.underway = None,
            Some(underway) => underway.resolves_at = None,
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
            Some(InProgress {
                kind: ActionKind::Attack,
                target: ActionTarget::Unit(target),
                ..
            }) => Some(target),
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
}
