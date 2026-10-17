use std::ops::Range;

use campfire_common::Tick;

use crate::player_input::PlayerInput;
use crate::session_log::SessionLog;
use crate::session_log::input_schedule::InputSchedule;
use crate::slot_change::SlotChange;

/// A replay of a log's sealed ticks, from tick 0: the next tick it seals, a schedule of its own
/// that the log's entries drive again, as they were logged, and the changes of a slot's
/// controller in the tick it sealed last. Its session owns it beside the log, which it reads at
/// each tick and never holds, and drops it once it reaches the log's next tick, where the log's
/// own schedule goes on live, so the log holds no replay of its own
/// (10-sessions.md, The replay is a cursor the session owns).
#[derive(Debug)]
pub struct LogReplay {
    next: Tick,
    schedule: InputSchedule,
    sealed_changes: Range<usize>,
}

impl LogReplay {
    /// The replay of `log`'s sealed ticks from tick 0; none for a log of no sealed tick, whose
    /// next tick its own schedule runs live.
    pub fn of(log: &SessionLog) -> Option<Self> {
        (log.next_tick() > Tick::ZERO).then(|| Self {
            next: Tick::ZERO,
            schedule: InputSchedule::new(log.control.len(), log.header.terms.inputs),
            sealed_changes: 0..0,
        })
    }

    /// The tick it seals next.
    pub const fn next_tick(&self) -> Tick {
        self.next
    }

    /// Whether it reached `log`'s next tick, from where the log runs live.
    pub fn done(&self, log: &SessionLog) -> bool {
        self.next == log.next_tick()
    }

    /// Seals its next tick, one `log` sealed, as the log sealed it when it took the tick's
    /// entries: the inputs it applied, by slot, then in the order logged, which no check needs
    /// again; `sealed_changes` then gives the changes of a slot's controller in it.
    pub fn seal_tick<'l>(
        &mut self,
        log: &'l SessionLog,
    ) -> impl ExactSizeIterator<Item = PlayerInput<'l>> {
        self.seal(log);
        let entries = &log.entries;
        self.schedule
            .due()
            .iter()
            .map(|&index| entries.input(index))
    }

    /// Seals each tick before `tick`, the schedule's alone, as a replay does that a checkpoint's
    /// snapshot starts at `tick`.
    pub fn seal_until(&mut self, log: &SessionLog, tick: Tick) {
        while self.next < tick {
            self.seal(log);
        }
    }

    /// The changes of a slot's controller that apply in the tick it sealed last.
    pub fn sealed_changes<'l>(&self, log: &'l SessionLog) -> &'l [SlotChange] {
        &log.changes[self.sealed_changes.clone()]
    }

    /// Queues again the entries `log` took before its next tick ran, and has the schedule give
    /// the inputs applied in it.
    fn seal(&mut self, log: &SessionLog) {
        debug_assert!(
            !self.done(log),
            "a replay seals only the ticks the log sealed"
        );
        let tick = self.next;
        let start = match tick.get().checked_sub(1) {
            Some(before) => log.entries.tick_end(Tick::new(before)),
            None => 0,
        };
        for at in start..log.entries.tick_end(tick) {
            self.schedule.replay(&log.entries, at, tick);
        }
        let first = log.changes.partition_point(|change| change.tick < tick);
        let after = log.changes.partition_point(|change| change.tick <= tick);
        self.sealed_changes = first..after;
        self.schedule.seal(tick);
        self.next = Tick::new(tick.get() + 1);
    }
}
