/// A tool of the server, parsed once from the name a call gives (02-engine-core.md, Tools by
/// outcome, by what they act on): `game_` acts on the processes, `sim_` on the match, `view_` on
/// the client's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl ToolName {
    /// Every tool, in the one order `tools/list` gives.
    pub(crate) const ALL: [Self; 9] = [
        Self::GameStart,
        Self::GameStop,
        Self::GameEvents,
        Self::SimStatus,
        Self::SimUnits,
        Self::SimRun,
        Self::SimPace,
        Self::ViewScreenshot,
        Self::ViewCamera,
    ];

    /// Its name, as MCP's calls give it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::GameStart => "game_start",
            Self::GameStop => "game_stop",
            Self::GameEvents => "game_events",
            Self::SimStatus => "sim_status",
            Self::SimUnits => "sim_units",
            Self::SimRun => "sim_run",
            Self::SimPace => "sim_pace",
            Self::ViewScreenshot => "view_screenshot",
            Self::ViewCamera => "view_camera",
        }
    }

    /// The tool of `name`; none for a name of no tool.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }
}
