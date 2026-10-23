use bevy_ecs::change_detection::Mut;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_actions::{ActionBook, ActionSlots, Cut, SlotSelect, SlotTimes};
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::{SimWorld, StableId};
use campfire_values::ActionId;

/// A change to units' cooldowns or ranks that a call queued.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AbilitiesEffect {
    /// `cut` off the cooldown of each of the unit's slots that `slots` picks.
    CutCooldown {
        unit: StableId,
        slots: SlotSelect,
        cut: Cut,
    },
    /// A charge more of the unit's action `action`, which it holds and which has charges.
    AddCharge { unit: StableId, action: ActionId },
    /// A rank more of the ability in the unit's slot `slot`, which has a rank above its rank.
    Learn { unit: StableId, slot: u8 },
}

impl Effect for AbilitiesEffect {
    const KIND: EffectKind = EffectKind::Change;

    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let unit = match self {
            Self::CutCooldown { unit, .. }
            | Self::AddCharge { unit, .. }
            | Self::Learn { unit, .. } => unit,
        };
        let Some(entity) = world.entity_of(unit) else {
            return;
        };
        let book = world.resource::<ActionBook>().clone();
        let mut held = world.entity_mut(entity);
        match self {
            Self::CutCooldown {
                slots: select, cut, ..
            } => {
                let (slots, mut times) = timed(&mut held);
                times.cut(slots, select, cut, now);
            }
            Self::AddCharge { action, .. } => {
                let (slots, mut times) = timed(&mut held);
                times.add_charge(slots, action, &book);
            }
            Self::Learn { slot, .. } => {
                let slots = held.get_mut::<ActionSlots>();
                slots
                    .expect("a unit the view held the slot of has slots")
                    .learn(slot);
            }
        }
    }
}

/// The slots and their times of a unit the view held with an ability.
fn timed<'w>(held: &'w mut EntityWorldMut<'_>) -> (&'w ActionSlots, Mut<'w, SlotTimes>) {
    held.get_components_mut::<(&ActionSlots, &mut SlotTimes)>()
        .expect("a unit the view held with the ability has slots")
}
