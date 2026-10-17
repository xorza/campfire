use campfire_actions::LoadoutRanks;
use campfire_declare::{BookCx, BookError, Declarations, InstallCx, Module, NoSet};
use campfire_sim::Capability;
use campfire_units::TeamNames;

use crate::match_end::MatchEnd;
use crate::mode_data::ModeData;
use crate::mode_map::ModeMap;
use crate::mode_units::ModeUnits;

/// The part of the mode every match installs, whatever its role: the match's end, and the stop
/// of every stage once it comes, as a client holds the end the server sends. The scripted part,
/// `Mode`, installs once the scripts compile.
#[derive(Debug)]
pub struct ModeCore;

impl Module for ModeCore {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = NoSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.fact::<TeamNames>(ModeMap::team_fact);
        d.fact::<LoadoutRanks>(Self::loadout_ranks);
        d.fact::<ModeMap>(ModeMap::build);
        d.fact::<ModeUnits>(ModeUnits::build);
        d.component::<MatchEnd>();
        d.stages_run_if(MatchEnd::running);
    }

    fn install(_: &mut InstallCx<'_, Self::Set>) {}
}

impl ModeCore {
    /// The ranks of every loadout entry, as the mode's data gives them.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    fn loadout_ranks(cx: &BookCx<'_>) -> Result<LoadoutRanks, BookError> {
        let data = cx
            .given::<ModeData>()
            .expect("the load gives the mode's data");
        Ok(LoadoutRanks(data.loadout_ranks()))
    }
}
