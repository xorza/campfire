#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use campfire_console::{
    CameraAsk, Detail, EventLevel, GameSpeed, RunTo, ShotRegion, UnitSelection,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::game_kind::{BuildProfile, GameKind};

/// `game_start`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct StartParams {
    /// The game to start: the client, whose `--local` plays a match of its own, or the dedicated
    /// server.
    pub(crate) game: GameKind,
    /// The game's command line, as its `--help` gives it, paths from the workspace's `source`
    /// directory: for a local match, `--local --data <directory> <mode directory>`; the console's
    /// own flag is added.
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// The profile it builds in: fast, as a player plays, unless named.
    pub(crate) profile: Option<BuildProfile>,
}

/// The arguments of a call that names a game alone.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GameParams {
    /// The game's handle, as `game_start` gave it; none for the only game.
    pub(crate) handle: Option<u32>,
}

/// `game_events`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct EventsParams {
    /// The game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// The lowest level it takes: `info` unless named.
    pub(crate) level: Option<EventLevel>,
    /// A prefix of the targets it takes, as `campfire_runner`.
    pub(crate) target: Option<String>,
    /// The first tick it takes, of events that name a tick.
    pub(crate) since_tick: Option<u64>,
    /// The most events, the latest: 50 unless named.
    pub(crate) limit: Option<u32>,
}

/// `sim_units`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UnitsParams {
    /// The game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// The units the table holds: all unless named.
    pub(crate) units: Option<UnitSelection>,
    /// Its columns: concise unless named.
    pub(crate) detail: Option<Detail>,
    /// Its first row, counted from 0: 0 unless named.
    pub(crate) first: Option<u32>,
    /// The most rows: 100 unless named.
    pub(crate) limit: Option<u32>,
}

/// `sim_run`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RunParams {
    /// The game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// Where the run ends: after a tick, after a count of ticks from the tick the call reads
    /// as it starts, or after the first tick at whose end a condition holds.
    pub(crate) to: RunTo,
    /// The most seconds the run takes: 110 unless named.
    pub(crate) wall_seconds: Option<u32>,
}

/// `sim_pace`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PaceParams {
    /// The game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// Pauses, or plays on.
    pub(crate) paused: Option<bool>,
    /// The speed it plays at.
    pub(crate) speed: Option<GameSpeed>,
}

/// `view_screenshot`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ShotParams {
    /// The client game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// The part of the window: the whole unless named, a rectangle of the last screenshot's
    /// pixels, or the box round units on the screen.
    pub(crate) region: Option<ShotRegion>,
}

/// `view_camera`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CameraParams {
    /// The client game's handle; none for the only game.
    pub(crate) handle: Option<u32>,
    /// What it does with the camera: reads it unless named, sets its pose, or looks at a unit
    /// or a point of the ground.
    pub(crate) camera: Option<CameraAsk>,
}
