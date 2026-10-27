//! The console's protocol (console.md): the lines a game and the tool that started it exchange
//! over the game's standard streams, a JSON object each, with the fingerprint of their types. It
//! holds no engine, so a tool builds it without the sim.

mod camera_ask;
mod console_event;
mod console_opening;
mod console_refusal;
mod fingerprint;
mod nearest_names;
mod output;
mod pace_ask;
mod reply;
mod request;
mod run_ask;
mod session_status;
mod shot;
mod shot_ask;
mod unit_table;
mod units_ask;

pub use crate::camera_ask::{CameraAsk, CameraPose};
pub use crate::console_event::{ConsoleEvent, EventLevel, FieldValue};
pub use crate::console_opening::{ConsoleOpening, GameRole};
pub use crate::console_refusal::ConsoleRefusal;
pub use crate::fingerprint::Fingerprint;
pub use crate::nearest_names::NearestNames;
pub use crate::output::Output;
pub use crate::pace_ask::PaceAsk;
pub use crate::reply::{ConsoleAnswer, Reply};
pub use crate::request::{ConsoleAsk, Request, RequestId};
pub use crate::run_ask::{RunAsk, RunCondition, RunStop, RunTo};
pub use crate::session_status::{
    FailedScriptCall, GameSpeed, PaceStatus, PlayerStatus, ScriptFailureCount, SessionPhase,
    SessionStatus,
};
pub use crate::shot::WindowShot;
pub use crate::shot_ask::{ShotAsk, ShotRegion};
pub use crate::unit_table::UnitTable;
pub use crate::units_ask::{Detail, UnitActivity, UnitSelection, UnitsAsk};

#[cfg(test)]
mod tests;
