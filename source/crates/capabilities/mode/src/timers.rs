use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_script::StateValue;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

/// The mode's timers, earliest first. Times count ticks from the match start, as
/// `SimTick::start` and `SimTick::end` give them: the Mode stage is at its tick's end, so a timer
/// never fires early.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timers {
    /// Sorted by due time, then by when each was set, the latest first: the earliest fires from
    /// the end.
    pub timers: Vec<Timer>,
    /// Orders timers due at the same time by when they were set.
    pub next_seq: u64,
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
    pub fn set(
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
        self.insert(timer);
    }

    /// Puts `timer`, the last set, among the others: after those due later, before those due
    /// with it.
    fn insert(&mut self, timer: Timer) {
        let at = self.timers.partition_point(|held| held.due > timer.due);
        self.timers.insert(at, timer);
    }

    /// The earliest timer, when it is due at `now`.
    pub fn due(&self, now: Tick) -> Option<&Timer> {
        self.timers.last().filter(|timer| timer.due <= now)
    }

    /// Ends the earliest timer, which sets itself again when it repeats.
    pub fn fire(&mut self) {
        let mut timer = self.timers.pop().expect("a timer fires once due");
        if let Some(every) = timer.every {
            timer.due = timer.due.after(every);
            timer.seq = self.next_seq;
            self.next_seq += 1;
            self.insert(timer);
        }
    }
}

impl SimResource for Timers {
    const NAME: &'static str = "mode.timers";

    // The earliest timer fires first only while they keep their order, and a new one is set last
    // among those due with it only while its number is the highest. A repeating timer of no
    // ticks would be due again as it fires, in the same tick, without end.
    fn check(&self, _: &World) -> bool {
        let ordered = self
            .timers
            .windows(2)
            .all(|two| (two[0].due, two[0].seq) > (two[1].due, two[1].seq));
        let each = self.timers.iter().all(|timer| {
            let repeats = timer.every.is_none_or(|every| every >= Ticks::ONE);
            timer.seq < self.next_seq && repeats
        });
        ordered && each && self.next_seq <= Tick::LIMIT.get()
    }
}
