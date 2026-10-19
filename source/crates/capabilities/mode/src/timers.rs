use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_script::{ListPlace, MatchNames, StateValue};
use campfire_sim::SimResource;
use campfire_values::NameId;
use serde::{Deserialize, Serialize};

use crate::match_mode::MatchMode;
use crate::timer_id::TimerId;

/// The mode's timers, earliest first, each of the script that set it (03-game-scripting.md, Rules
/// scripts in order). Times count ticks from the match start, as `SimTick::start` and
/// `SimTick::end` give them: the Mode stage is at its tick's end, so a timer never fires early.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timers {
    /// Sorted by due time, then by when each was set, the latest first: the earliest fires from
    /// the end.
    timers: Vec<Timer>,
    /// Orders timers due at the same time by when they were set.
    next_seq: u64,
}

/// A timer: the place in the mode's list of the script that set it, whose `on_timer` receives its
/// name and data, when it is due, and its period if it repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    pub owner: ListPlace,
    pub name: TimerId,
    pub due: Tick,
    seq: u64,
    /// In ticks, when it repeats.
    pub every: Option<Ticks>,
    /// `None` for `()`.
    pub data: Option<StateValue>,
}

/// A timer a call sets: the place in the mode's list of the call's script, the name and data its
/// `on_timer` receives, the ticks until it is first due, and its period, when it repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerStart {
    pub owner: ListPlace,
    pub name: TimerId,
    pub after: Ticks,
    pub every: Option<Ticks>,
    /// `None` for `()`.
    pub data: Option<StateValue>,
}

impl Timers {
    /// Sets the timer `start` gives, `after` ticks from `now`.
    pub fn set(&mut self, now: Tick, start: TimerStart) {
        let TimerStart {
            owner,
            name,
            after,
            every,
            data,
        } = start;
        let timer = Timer {
            owner,
            name,
            due: now.after(after),
            seq: self.next_seq,
            every,
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

    /// Ends every timer of `name` the script at `owner` set, the ones a call set before it among
    /// them.
    pub fn cancel(&mut self, owner: ListPlace, name: TimerId) {
        self.timers
            .retain(|timer| (timer.owner, timer.name) != (owner, name));
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
    // ticks would be due again as it fires, in the same tick, without end. A name the mode does
    // not declare has no text for `on_timer`, and a script past the mode's list no `on_timer`.
    fn check(&self, world: &World) -> bool {
        let names = world.resource::<MatchNames>().count::<TimerId>();
        let mode = world.get_resource::<MatchMode>();
        let scripts = mode.map_or(0, |mode| mode.schema.scripts.len());
        let ordered = self
            .timers
            .windows(2)
            .all(|two| (two[0].due, two[0].seq) > (two[1].due, two[1].seq));
        let each = self.timers.iter().all(|timer| {
            let repeats = timer.every.is_none_or(|every| every >= Ticks::ONE);
            let named = timer.name.index() < names && timer.owner.index() < scripts;
            timer.seq < self.next_seq && repeats && named
        });
        ordered && each && self.next_seq <= Tick::LIMIT.get()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::timers::{Timer, Timers};

    impl Timers {
        /// The timers, to change as a flawed snapshot holds them.
        pub const fn timers_mut(&mut self) -> &mut Vec<Timer> {
            &mut self.timers
        }

        /// The sequence number the next timer takes, to change as a flawed snapshot holds it.
        pub const fn next_seq_mut(&mut self) -> &mut u64 {
            &mut self.next_seq
        }
    }
}
