use campfire_common::NamedEnum;

/// A tool of the server, parsed once from the name a call gives, its members in the one order
/// `tools/list` gives (02-engine-core.md, Tools by
/// outcome, by what they act on): `game_` acts on the processes, `sim_` on the match, `view_` on
/// the client's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub(crate) enum ToolName {
    GameStart,
    GameStop,
    GameEvents,
    SimStatus,
    SimUnits,
    SimRun,
    SimPace,
    ViewScreenshot,
    ViewCamera,
}
