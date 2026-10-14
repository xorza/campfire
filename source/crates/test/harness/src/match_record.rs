use campfire_capabilities::Match;
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;

use crate::copy_check::CopyCheck;
use crate::golden::Golden;
use crate::hash_trail::HashTrail;
use campfire_runner::Replay;

/// The record of a match a test plays, tick by tick: a copy of its state checked after each tick,
/// the trail of its hashes, which the replay of its log must give again, and its golden record.
/// The state is hashed once a tick, which the three share.
#[derive(Debug)]
pub struct MatchRecord<'p> {
    packages: &'p ModePackages,
    copy: CopyCheck,
    trail: HashTrail,
    golden: Golden,
}

impl<'p> MatchRecord<'p> {
    /// The record of `game`, a match of `packages` for `players` players, from its state as it
    /// stands.
    pub fn of(game: &mut Match, packages: &'p ModePackages, players: u32) -> MatchRecord<'p> {
        MatchRecord {
            packages,
            copy: CopyCheck::of(game),
            trail: HashTrail::default(),
            golden: Golden::new(packages, players),
        }
    }

    /// Records the tick `game` just ran, once its copy checks.
    pub fn tick(&mut self, game: &mut Match) {
        let hashes = self.copy.check(game);
        self.trail.record_hashes(hashes);
        self.golden.record(game, hashes.total);
    }

    /// Checks the golden record of the ticks recorded against the one pinned as `name`.
    pub fn check(&self, name: &str) {
        self.golden.check(name);
    }

    /// Asserts that `log`, the match's published log, replays from its file, checkpoints and
    /// result included, to the hash of every tick recorded, and ends where the match ended.
    pub fn assert_replays(&self, log: &SessionLog) {
        let mut file = Vec::new();
        log.encode(&mut file);
        let published = SessionLog::decode(&file).expect("a log decodes from its file");
        let mut replay = Replay::of(published, self.packages).expect("the log's match starts");
        let replayed =
            HashTrail::of_replay(&mut replay).expect("the log replays to what it records");
        self.trail.assert_same(&replayed);
    }
}
