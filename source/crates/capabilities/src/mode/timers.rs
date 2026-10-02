use bevy_ecs::resource::Resource;
use campfire_math::{Tick, Ticks};
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::scripts::state_value::StateValue;

/// The mode's timers, earliest first. Times count ticks from the match start, as
/// `SimTick::start` and `SimTick::end` give them: the Mode stage is at its tick's end, so a timer
/// never fires early.
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
    pub due: Tick,
    seq: u64,
    /// In ticks, when it repeats.
    pub every: Option<Ticks>,
    /// `None` for `()`.
    pub data: Option<StateValue>,
}

impl Timers {
    /// Sets a timer `ticks` after `now`, and again every `ticks` if it repeats.
    pub(crate) fn set(
        &mut self,
        now: Tick,
        name: String,
        ticks: Ticks,
        repeat: bool,
        data: Option<StateValue>,
    ) {
        let due = now.after(ticks);
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
    pub(crate) fn due(&self, now: Tick) -> Option<&Timer> {
        self.timers.first().filter(|timer| timer.due <= now)
    }

    /// Takes the earliest timer, and sets it again when it repeats.
    pub(crate) fn fire(&mut self) -> Timer {
        let timer = self.timers.remove(0);
        if let Some(every) = timer.every {
            let again = timer.due.after(every);
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
