use campfire_common::{MapName, PlayerSlot, Tick, Version};
use campfire_package::PackageName;
use campfire_protocol::SessionId;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// The facts of a player's report, its `report.toml`: the engine release, the mode and the map,
/// the session and the tick the client drew, the player slot it plays in, the units it selected,
/// its camera, and its window's size in pixels (bug-reports.md, A report is a folder of plain
/// files).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportFile {
    pub release: Version,
    pub mode: PackageName,
    pub map: MapName,
    pub session: SessionId,
    pub tick: Tick,
    pub player_slot: Option<PlayerSlot>,
    pub selected: Vec<StableId>,
    pub camera: ReportCamera,
    pub window: [u32; 2],
}

/// A report's camera: its rig's pivot on the ground, yaw and pitch in radians, and height,
/// which put it back exactly, and for a reader its place and its forward in the world
/// (bug-reports.md, The camera is the rig's state).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportCamera {
    pub pivot: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
    pub height: f32,
    pub place: [f32; 3],
    pub forward: [f32; 3],
}
