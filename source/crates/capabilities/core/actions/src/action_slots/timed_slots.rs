//! A unit's slots with their times, a bundle of its own.
use bevy_ecs::bundle::Bundle;

use crate::action_slots::ActionSlots;
use crate::action_slots::action_progress::ActionProgress;
use crate::action_slots::slot_times::SlotTimes;

/// A unit's slots with their times, each ready at once, and nothing under way, as a unit gains
/// slots.
#[derive(Bundle, Debug)]
pub struct TimedSlots {
    times: SlotTimes,
    progress: ActionProgress,
    slots: ActionSlots,
}

impl TimedSlots {
    pub fn new(slots: ActionSlots) -> Self {
        Self {
            times: SlotTimes::ready(&slots),
            progress: ActionProgress::default(),
            slots,
        }
    }
}
