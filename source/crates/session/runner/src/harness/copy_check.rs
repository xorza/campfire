use std::fmt::Write as _;

use campfire_capabilities::Match;
use campfire_sim::{SimWorld, StateCopy, StateRegistry, TypeHash};

use crate::harness::hash_trail::MatchHashes;
use crate::session::session_world::SessionWorld;

/// A `StateCopy` of a match that follows it tick by tick, checked after each: every state type
/// of the copy hashes as the match's own, so the copy of what changed missed no write.
#[derive(Debug)]
pub(crate) struct CopyCheck {
    copy: StateCopy,
    /// Each type's hash in the copy and in the match, kept between ticks.
    copied: Vec<TypeHash>,
    own: Vec<TypeHash>,
}

impl CopyCheck {
    /// A copy of the state of `game` as it stands.
    pub(crate) fn of(game: &mut Match) -> CopyCheck {
        CopyCheck {
            copy: StateCopy::of(game),
            copied: Vec::new(),
            own: Vec::new(),
        }
    }

    /// Brings the copy up to `game`, and panics naming each type whose hash differs; the
    /// match's hashes, which a `HashTrail` and a `Golden` record, so a tick hashes it once.
    pub(crate) fn check(&mut self, game: &mut Match) -> MatchHashes<'_> {
        self.copy.follow(game);
        let registry = game.resource::<StateRegistry>();
        registry.hash_by_type(self.copy.world(), &mut self.copied);
        let total = game.state_hash_by_type(&mut self.own);
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
            game.session().log().next_tick().get().saturating_sub(1)
        );
    }
}
