use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::{EntityIndex, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::units::action_id::ActionId;

/// A change to units' cooldowns that a call queued.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AbilitiesEffect {
    /// `cut` off the cooldown of the unit's action `action`, which it holds.
    ReduceCooldown {
        unit: StableId,
        action: ActionId,
        cut: Ticks,
    },
    /// `fraction`, from 0 to 1, of what is left of the cooldown of each of the unit's actions in
    /// `kind`.
    ReduceCooldowns {
        unit: StableId,
        kind: SlotKind,
        fraction: Num,
    },
    /// A charge more of the unit's action `action`, which it holds and which has charges.
    AddCharge { unit: StableId, action: ActionId },
}

impl Effect for AbilitiesEffect {
    fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
        let unit = match self {
            AbilitiesEffect::ReduceCooldown { unit, .. }
            | AbilitiesEffect::ReduceCooldowns { unit, .. }
            | AbilitiesEffect::AddCharge { unit, .. } => unit,
        };
        let Some(entity) = world.resource::<EntityIndex>().get(unit) else {
            return;
        };
        world.resource_scope(|world, book: Mut<'_, ActionBook>| {
            let mut slots = world
                .get_mut::<ActionSlots>(entity)
                .expect("a unit the view held with the ability has slots");
            match self {
                AbilitiesEffect::ReduceCooldown { action, cut, .. } => {
                    slots.cut_cooldown(action, cut, now);
                }
                AbilitiesEffect::ReduceCooldowns { kind, fraction, .. } => {
                    slots.cut_cooldowns(kind, fraction, now);
                }
                AbilitiesEffect::AddCharge { action, .. } => slots.add_charge(action, &book),
            }
        });
    }
}
