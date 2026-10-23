use campfire_capabilities::Match;
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;

use crate::golden::Golden;
use crate::replay_record::ReplayRecord;

/// The record of a match a test plays, tick by tick: its record for the replay, and its golden
/// record. The state is hashed once a tick, which the two share.
#[derive(Debug)]
pub struct MatchRecord<'p> {
    replay: ReplayRecord<'p>,
    golden: Golden,
}

impl<'p> MatchRecord<'p> {
    /// The record of `game`, a match of `packages` for `players` players, from its state as it
    /// stands.
    pub fn of(game: &mut Match, packages: &'p ModePackages, players: u32) -> Self {
        MatchRecord {
            replay: ReplayRecord::of(game, packages),
            golden: Golden::new(packages, players),
        }
    }

    /// Records the tick `game` just ran, once its copy checks.
    pub fn tick(&mut self, game: &mut Match) {
        let total = self.replay.tick(game);
        self.golden.record(game, total);
    }

    /// Checks the golden record of the ticks recorded against the one pinned as `name`.
    pub fn check(&self, name: &str) {
        self.golden.check(name);
    }

    /// Compares the golden record of the ticks recorded with the one pinned as `name`, which
    /// another test of the same match writes; it never writes the file.
    pub fn compare(&self, name: &str) {
        self.golden.compare(name);
    }

    /// Asserts that each segment of `log` replays to the hashes recorded, as
    /// `ReplayRecord::assert_segments_replay` says.
    pub fn assert_segments_replay(&self, log: &SessionLog, snapshots: &[Vec<u8>]) {
        self.replay.assert_segments_replay(log, snapshots);
    }
}
