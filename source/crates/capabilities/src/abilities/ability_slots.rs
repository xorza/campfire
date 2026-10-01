use bevy_ecs::component::Component;
use campfire_sim::{SimComponent, StableId, Tick};
use serde::{Deserialize, Serialize};

use crate::abilities::ability_book::AbilityId;
use crate::abilities::slot_kind::SlotKind;

/// A unit's abilities: its slots, kind after kind in the mode's order, each an ability at a rank
/// with its cooldown, and the cast it was ordered or is casting.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySlots {
    slots: Vec<AbilitySlot>,
    casting: Option<Casting>,
}

/// One slot: the ability, its kind, its rank, 0 while not learned, and the first tick it may be
/// cast again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySlot {
    pub ability: AbilityId,
    pub kind: SlotKind,
    pub rank: u8,
    pub ready_at: Tick,
}

/// A cast of the ability in `slot` at `target`: ordered and not checked yet while `resolves_at`
/// is `None`, then started, to resolve in that tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Casting {
    pub(crate) slot: u8,
    pub(crate) target: CastTarget,
    pub(crate) resolves_at: Option<Tick>,
}

/// What a cast is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastTarget {
    None,
    Unit(StableId),
}

impl AbilitySlots {
    /// The most slots a unit holds: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    /// Slots of `(ability, kind, rank)`, in the order of their kinds, each ready at once.
    pub fn new(slots: impl IntoIterator<Item = (AbilityId, SlotKind, u8)>) -> AbilitySlots {
        let slots: Vec<AbilitySlot> = slots
            .into_iter()
            .map(|(ability, kind, rank)| AbilitySlot {
                ability,
                kind,
                rank,
                ready_at: Tick::ZERO,
            })
            .collect();
        debug_assert!(slots.is_sorted_by_key(|slot| slot.kind));
        AbilitySlots {
            slots,
            casting: None,
        }
    }

    /// Puts `abilities` in `kind` at `rank`, each ready at once, after the slots of that kind it
    /// has: the slots of later kinds, and a cast ordered from one, move along.
    pub(crate) fn grant(&mut self, kind: SlotKind, abilities: &[AbilityId], rank: u8) {
        let at = self.slots.partition_point(|slot| slot.kind <= kind);
        let added = abilities.iter().map(|&ability| AbilitySlot {
            ability,
            kind,
            rank,
            ready_at: Tick::ZERO,
        });
        self.slots.splice(at..at, added);
        if let Some(casting) = &mut self.casting
            && usize::from(casting.slot) >= at
        {
            let moved = usize::from(casting.slot) + abilities.len();
            casting.slot = u8::try_from(moved).expect("a unit's slots fit u8");
        }
    }

    pub fn slot(&self, slot: u8) -> Option<AbilitySlot> {
        self.slots.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = AbilitySlot> + '_ {
        self.slots.iter().copied()
    }

    /// Raises the ability in `slot` a rank; the caller checked it has one more.
    pub(crate) fn learn(&mut self, slot: u8) {
        let slot = &mut self.slots[usize::from(slot)];
        slot.rank = slot
            .rank
            .checked_add(1)
            .expect("a rank below the ability's ranks");
    }

    /// Whether a cast was ordered or is under way.
    pub const fn is_casting(&self) -> bool {
        self.casting.is_some()
    }

    /// Orders a cast, in place of any other not resolved yet.
    pub(crate) const fn order(&mut self, slot: u8, target: CastTarget) {
        self.casting = Some(Casting {
            slot,
            target,
            resolves_at: None,
        });
    }

    pub(crate) const fn casting(&self) -> Option<Casting> {
        self.casting
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept.
    pub(crate) const fn start(&mut self, resolves_at: Tick, target: CastTarget) {
        if let Some(casting) = &mut self.casting {
            casting.resolves_at = Some(resolves_at);
            casting.target = target;
        }
    }

    /// Takes the cast back to its order, which starts it again from its check, and spends
    /// nothing.
    pub(crate) const fn interrupt(&mut self) {
        if let Some(casting) = &mut self.casting {
            casting.resolves_at = None;
        }
    }

    /// Ends the cast, resolved or not.
    pub(crate) const fn stop(&mut self) {
        self.casting = None;
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        if let Some(slot) = self.slots.get_mut(usize::from(slot)) {
            slot.ready_at = ready_at;
        }
    }
}

impl SimComponent for AbilitySlots {
    const NAME: &'static str = "abilities.slots";
}
