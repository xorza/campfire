use bevy_ecs::component::Component;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::abilities::ability_book::AbilityId;

/// A unit's abilities: its slots, each an ability at a rank with its cooldown, and the cast it
/// was ordered or is casting.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySlots {
    slots: Vec<AbilitySlot>,
    casting: Option<Casting>,
}

/// One slot: the ability, its rank, 0 while not learned, and the first tick it may be cast again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySlot {
    pub ability: AbilityId,
    pub rank: u8,
    pub ready_at: u64,
}

/// A cast of the ability in `slot` at `target`: ordered and not checked yet while `resolves_at`
/// is `None`, then started, to resolve in that tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Casting {
    pub(crate) slot: u8,
    pub(crate) target: CastTarget,
    pub(crate) resolves_at: Option<u64>,
}

/// What a cast is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastTarget {
    None,
    Unit(StableId),
}

impl AbilitySlots {
    /// Slots of `(ability, rank)`, each ready at once.
    pub fn new(slots: impl IntoIterator<Item = (AbilityId, u8)>) -> AbilitySlots {
        AbilitySlots {
            slots: slots
                .into_iter()
                .map(|(ability, rank)| AbilitySlot {
                    ability,
                    rank,
                    ready_at: 0,
                })
                .collect(),
            casting: None,
        }
    }

    pub fn slot(&self, slot: u8) -> Option<AbilitySlot> {
        self.slots.get(usize::from(slot)).copied()
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

    pub(crate) const fn start(&mut self, resolves_at: u64) {
        if let Some(casting) = &mut self.casting {
            casting.resolves_at = Some(resolves_at);
        }
    }

    /// Ends the cast, resolved or not.
    pub(crate) const fn stop(&mut self) {
        self.casting = None;
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, slot: u8, ready_at: u64) {
        if let Some(slot) = self.slots.get_mut(usize::from(slot)) {
            slot.ready_at = ready_at;
        }
    }
}

impl SimComponent for AbilitySlots {
    const NAME: &'static str = "abilities.slots";
}
