use campfire_capabilities::Match;
use campfire_common::StateHash;
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;
use campfire_runner::{Replay, ResumeAt};
use campfire_threads::Parallel;

use crate::copy_check::CopyCheck;
use crate::hash_trail::HashTrail;

/// The record of a match a test or a check plays, tick by tick, for its replay: a copy of its
/// state checked after each tick, and the trail of its hashes, which the replay of its log must
/// give again.
#[derive(Debug)]
pub struct ReplayRecord<'p> {
    packages: &'p ModePackages,
    copy: CopyCheck,
    trail: HashTrail,
}

impl<'p> ReplayRecord<'p> {
    /// The record of `game`, a match of `packages`, from its state as it stands.
    pub fn of(game: &mut Match, packages: &'p ModePackages) -> Self {
        ReplayRecord {
            packages,
            copy: CopyCheck::of(game),
            trail: HashTrail::default(),
        }
    }

    /// Records the tick `game` just ran, once its copy checks; its state's hash.
    pub fn tick(&mut self, game: &mut Match) -> StateHash {
        let hashes = self.copy.check(game);
        self.trail.record_hashes(hashes);
        hashes.total
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
}
