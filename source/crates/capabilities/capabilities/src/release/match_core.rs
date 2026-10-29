use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::{Local, Res, ResMut};
use campfire_common::EnumAll;
use campfire_declarations::{
    Declarations, InstallCx, MatchFacts, Module, ModuleSystems, NameSets, NamedScripts, NamedTexts,
    SetPlace, StateFields,
};
use campfire_mode::MatchMode;
use campfire_players::Select;
use campfire_sim::{
    Addressee, Capability, SimEdgeSystems, SimWorldBuilder, StableId, TickInputs, UnreadCause,
    UnreadCommands,
};

use crate::order::Directive;

/// What a match's build puts in itself before any module installs: the sim's world, and the
/// match's facts; and the facts of what the packages' data names, which the modules add to. The
/// first module of the release's list, so the audit of a match's resources reads them as it reads
/// every other.
#[derive(Debug)]
pub(super) struct MatchCore;

/// The steps of the match's core.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub(super) enum MatchCoreSystems {
    /// Before Inputs: the tick's commands that nothing reads are counted.
    Unread,
}

impl ModuleSystems for MatchCoreSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Unread => SetPlace::gap(SimEdgeSystems::Start),
        }
    }
}

impl Module for MatchCore {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = MatchCoreSystems;

    fn declare<D: Declarations>(d: &mut D) {
        SimWorldBuilder::own_resources(d);
        d.own::<MatchFacts>();
        d.fact::<NamedScripts>(|_| Ok(NamedScripts::default()));
        d.fact::<NamedTexts>(|_| Ok(NamedTexts::default()));
        d.fact::<StateFields>(|_| Ok(StateFields::default()));
        d.fact::<NameSets>(|_| Ok(NameSets::default()));
        d.report::<UnreadCommands>();
    }

    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.server(MatchCoreSystems::Unread, Self::count_unread);
    }
}

impl MatchCore {
    /// Counts in the tick's report each command of the tick that nothing reads: a payload with a
    /// flaw, a body its addressee's reader does not decode, as it decodes it, and a command to an
    /// addressee no module of the match reads (02-engine-core.md, A command nothing reads is
    /// counted). The mode decodes its inputs by the names its schema declares, within its calls,
    /// so it counts those it does not read itself.
    fn count_unread(
        inputs: Res<'_, TickInputs>,
        facts: Res<'_, MatchFacts>,
        mode: Option<Res<'_, MatchMode>>,
        mut unread: ResMut<'_, UnreadCommands>,
        mut named: Local<'_, Vec<StableId>>,
    ) {
        unread.begin(&inputs);
        for (to, command) in inputs.all() {
            let body = command.body;
            let read = match to {
                Addressee::Players => {
                    named.clear();
                    Some(Select::decode_into(body, &mut named).is_some())
                }
                Addressee::Capability(Capability::Mode) => mode.as_ref().map(|_| true),
                Addressee::Capability(capability) => Directive::decoder(capability)
                    .filter(|_| facts.set.contains(capability))
                    .map(|decode| decode(body).is_some()),
            };
            let cause = match read {
                Some(true) => continue,
                Some(false) => UnreadCause::Undecodable(to),
                None => UnreadCause::Unaddressed(to),
            };
            unread.push(command.player_slot, cause);
        }
    }
}
