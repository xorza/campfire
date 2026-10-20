use std::num::NonZeroU32;

use campfire_package::ModePackages;

use crate::fixed_session::FixedSession;
use crate::fixed_session::fixed_match::FixedMatch;
use crate::scripted::{Plan, Scripted};

/// A mode a fixed session plays with scripted players, by the plan `P` of the mode: what each
/// player orders in each tick, and when a player joins or leaves.
#[derive(Debug)]
pub struct ScriptedMode<P> {
    session: FixedSession,
    script: Vec<Scripted<P>>,
}

impl<P: Plan> ScriptedMode<P> {
    pub(crate) const fn new(session: FixedSession, script: Vec<Scripted<P>>) -> Self {
        Self { session, script }
    }

    pub const fn packages(&self) -> &ModePackages {
        self.session.packages()
    }

    pub const fn session(&self) -> &FixedSession {
        &self.session
    }

    /// The mode on a seed chain of `segments` segments, as `FixedSession::segments` gives it.
    #[must_use]
    pub fn segments(mut self, segments: NonZeroU32) -> Self {
        self.session = self.session.segments(segments);
        self
    }

    /// A match at tick 0, after what the plan sends at the start.
    pub fn start(&self) -> FixedMatch {
        let mut fixed = self.session.start();
        P::start(&mut fixed, &self.session.terms().seats);
        fixed
    }

    /// Runs tick `tick` of `fixed`: first logs the joins and the leaves the script holds in it,
    /// then sends the scripted orders stamped for it, then runs it.
    pub fn play_tick(&self, fixed: &mut FixedMatch, tick: u64) {
        Scripted::play_tick(&self.script, fixed, tick);
    }
}
