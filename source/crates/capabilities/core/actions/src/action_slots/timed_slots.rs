//! A unit's slots with their times, a bundle of its own.
#![expect(
    clippy::mem_forget,
    reason = "Bevy's `Bundle` derive moves the fields out through `deconstruct_moving_ptr!`, which forgets the bundle"
)]

use bevy_ecs::bundle::Bundle;

use crate::action_slots::ActionSlots;
use crate::action_slots::slot_times::SlotTimes;

/// A unit's slots with their times, each ready at once, as a unit gains slots.
#[derive(Bundle, Debug)]
pub struct TimedSlots {
    times: SlotTimes,
    slots: ActionSlots,
}

impl TimedSlots {
    pub fn new(slots: ActionSlots) -> Self {
        Self {
            times: SlotTimes::ready(&slots),
            slots,
        }
    }
}
