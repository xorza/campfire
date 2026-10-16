use campfire_declare::{Declarations, InstallCx, Module};
use campfire_sim::Capability;

use crate::match_end::MatchEnd;

/// The part of the mode every match installs, whatever its role: the match's end, and the stop
/// of every stage once it comes, as a client holds the end the server sends. The scripted part,
/// `Mode`, installs once the books are in.
#[derive(Debug)]
pub struct ModeCore;

impl Module for ModeCore {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<MatchEnd>();
        d.stages_run_if(MatchEnd::running);
    }

    fn install(_: &mut InstallCx<'_>) {}
}
