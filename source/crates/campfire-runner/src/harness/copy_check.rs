use std::fmt::Write as _;

use campfire_sim::{StateCopy, TypeHash};

use crate::harness::hash_trail::MatchHashes;
use crate::runner::Runner;
use crate::session::Session;

/// A `StateCopy` of a match that follows it tick by tick, checked after each: every state type
/// of the copy hashes as the match's own, so the copy of what changed missed no write.
#[derive(Debug)]
pub struct CopyCheck {
    copy: StateCopy,
    /// Each type's hash in the copy and in the match, kept between ticks.
    copied: Vec<TypeHash>,
    own: Vec<TypeHash>,
}

impl CopyCheck {
    /// A copy of the state of `runner`'s match as it stands.
    pub fn new(runner: &mut Runner) -> CopyCheck {
        CopyCheck {
            copy: runner.copy_state(),
            copied: Vec::new(),
            own: Vec::new(),
        }
    }

    /// Brings the copy up to `runner`'s match, and panics naming each type whose hash differs;
    /// the match's hashes, which a `HashTrail` and a `Golden` record, so a tick hashes it once.
    pub fn check(&mut self, runner: &mut Runner) -> MatchHashes<'_> {
        runner.follow(&mut self.copy);
        let world = runner.world();
        let session = world.resource::<Session>();
        session.state_hash_by_type(self.copy.world(), &mut self.copied);
        let total = session.state_hash_by_type(world, &mut self.own);
        if self.copied == self.own {
            return MatchHashes {
                types: &self.own,
                total,
            };
        }
        let mut differ = String::new();
        for (copied, own) in self.copied.iter().zip(&self.own) {
            if copied.hash != own.hash {
                write!(differ, " {}", copied.name).expect("text writes into a string");
            }
        }
        panic!(
            "the copy differs from the match after tick {}:{differ}",
            runner.log().next_tick().get().saturating_sub(1)
        );
    }
}
