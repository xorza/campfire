use campfire_declare::{
    Declarations, InstallCx, MatchFacts, Module, NamedMessages, NamedScripts, NoSet, StateFields,
};
use campfire_sim::{Capability, SimWorldBuilder};

/// What a match's build puts in itself before any module installs: the sim's world, and the
/// match's facts; and the facts of what the packages' data names, which the modules add to. The
/// first module of the release's list, so the audit of a match's resources reads them as it reads
/// every other.
#[derive(Debug)]
pub(super) struct MatchCore;

impl Module for MatchCore {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = NoSet;

    fn declare<D: Declarations>(d: &mut D) {
        SimWorldBuilder::own_resources(d);
        d.own::<MatchFacts>();
        d.fact::<NamedScripts>(|_| Ok(NamedScripts::default()));
        d.fact::<NamedMessages>(|_| Ok(NamedMessages::default()));
        d.fact::<StateFields>(|_| Ok(StateFields::default()));
    }

    /// Adds no system: the sim's schedule holds its own.
    fn install(_: &mut InstallCx<'_, Self::Set>) {}
}
