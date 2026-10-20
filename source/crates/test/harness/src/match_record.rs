use campfire_capabilities::Match;
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;

use crate::copy_check::CopyCheck;
use crate::golden::Golden;
use crate::hash_trail::HashTrail;
use campfire_runner::{Replay, ResumeAt};
use campfire_threads::internals::Parallel;

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
    pub fn of(game: &mut Match, packages: &'p ModePackages, players: u32) -> Self {
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

    /// Compares the golden record of the ticks recorded with the one pinned as `name`, which
    /// another test of the same match writes; it never writes the file.
    pub fn compare(&self, name: &str) {
        self.golden.compare(name);
    }

    /// Asserts that each segment of `log`, the match's published log, replays apart, one on each
    /// thread, to the hash of every tick recorded, the checkpoint that ends it and the result
    /// included: the first from the whole file, every other from the file cut at its
    /// checkpoint, from `snapshots`' snapshot of it, by segment from the second
    /// (02-engine-core.md, A match's replay test replays its segments).
    pub fn assert_segments_replay(&self, log: &SessionLog, snapshots: &[Vec<u8>]) {
        let segments = log.segment() + 1;
        assert_eq!(
            snapshots.len(),
            usize::try_from(log.segment()).unwrap(),
            "a snapshot of each checkpoint"
        );
        let trails = Parallel::map(usize::try_from(segments).unwrap(), |segment| {
            let segment = u32::try_from(segment).unwrap();
            let mut file = Vec::new();
            log.encode_from(segment, &mut file);
            let published = SessionLog::decode(&file).expect("a log decodes from its file");
            let mut replay = match segment.checked_sub(1) {
                None => Replay::of(published, self.packages).expect("the log's match starts"),
                Some(before) => {
                    let at = ResumeAt {
                        segment,
                        snapshot: &snapshots[usize::try_from(before).unwrap()],
                    };
                    Replay::from_checkpoint(published, at, self.packages)
                        .expect("the log's match resumes from its checkpoint")
                }
            };
            HashTrail::of_segment(&mut replay).expect("the segment replays to what it records")
        });
        self.trail.assert_same(&trails.into_iter().collect());
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
