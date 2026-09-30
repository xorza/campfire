use bevy_ecs::resource::Resource;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::units::state_value::StateValue;

/// The mode's timers, earliest first. Times count ticks from the match start: the Mode stage of
/// tick `t` is at time `t + 1`, when that tick ends, so a timer never fires early.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timers {
    /// Sorted by due time, then by when each was set.
    timers: Vec<Timer>,
    /// Orders timers due at the same time by when they were set.
    next_seq: u64,
}

/// A timer: the name and data `on_timer` receives, when it is due, and its period if it repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    pub name: String,
    pub due: u64,
    seq: u64,
    /// In ticks, when it repeats.
    pub every: Option<u64>,
    /// `None` for `()`.
    pub data: Option<StateValue>,
}

impl Timers {
    /// Sets a timer `ticks` after `now`, and again every `ticks` if it repeats.
    pub(crate) fn set(
        &mut self,
        now: u64,
        name: String,
        ticks: u64,
        repeat: bool,
        data: Option<StateValue>,
    ) {
        let due = now.checked_add(ticks).expect("tick numbers exhausted");
        let timer = Timer {
            name,
            due,
            seq: self.next_seq,
            every: repeat.then_some(ticks),
            data,
        };
        self.next_seq += 1;
        let at = self.timers.partition_point(|held| held.due <= due);
        self.timers.insert(at, timer);
    }

    /// The earliest timer, when it is due at `now`.
    pub(crate) fn due(&self, now: u64) -> Option<&Timer> {
        self.timers.first().filter(|timer| timer.due <= now)
    }

    /// Takes the earliest timer, and sets it again when it repeats.
    pub(crate) fn fire(&mut self) -> Timer {
        let timer = self.timers.remove(0);
        if let Some(every) = timer.every {
            let again = timer
                .due
                .checked_add(every)
                .expect("tick numbers exhausted");
            let repeat = Timer {
                due: again,
                seq: self.next_seq,
                ..timer.clone()
            };
            self.next_seq += 1;
            let at = self.timers.partition_point(|held| held.due <= again);
            self.timers.insert(at, repeat);
        }
        timer
    }
}

impl SimResource for Timers {
    const NAME: &'static str = "mode.timers";
}
