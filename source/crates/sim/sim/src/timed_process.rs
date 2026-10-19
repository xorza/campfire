use bevy_ecs::query::{ROQueryItem, ReadOnlyQueryData};
use bevy_ecs::system::{ReadOnlySystemParam, SystemParamItem};
use campfire_common::Tick;

/// A process whose units come due at a tick, as a timed life ends or a respawn comes, which keeps
/// a `DueIndex` of them: the parts a unit's time is read from, and the tick it is due at, by
/// which the index builds again as the state is written from outside the sim, and a debug build
/// checks that each unit due is held (04-capabilities/00-overview.md, One due index for each
/// timed process).
pub trait TimedProcess: Send + Sync + 'static {
    /// Where its pass runs.
    const RUNS: TimedRuns;

    /// What pushes its units.
    const KEPT: TimedKept;

    /// The parts a unit's time is read from.
    type Parts: ReadOnlyQueryData + 'static;

    /// What besides the parts a unit's time is read from, as the books.
    type Books: ReadOnlySystemParam + 'static;

    /// The tick the unit of `parts` is due at, as `books` read it; none for one with no time to
    /// come.
    fn due(
        books: &SystemParamItem<'_, '_, Self::Books>,
        parts: ROQueryItem<'_, '_, Self::Parts>,
    ) -> Option<Tick>;
}

/// What pushes a timed process's units into its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimedKept {
    /// Each place that sets a unit's time, and a hook as its part comes: a debug build checks
    /// at each tick's end that each unit due is held.
    BySetters,
    /// Its own pass, which follows the changes of the parts its times derive from by their change
    /// ticks, as many writers change them, and pushes each unit it visits at its time: a time set
    /// after the pass is pushed by its next run, before any reader (04-capabilities/00-overview.md,
    /// One due index for each timed process).
    FollowsChanges,
}

/// Where a timed process's pass runs: on a match that computes the whole state alone, or on a
/// predicting client too, whose index follows its rollbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimedRuns {
    Server,
    EveryRole,
}
