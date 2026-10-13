use serde::Deserialize;

/// The backends of the core's collision, pathfinding and visibility. The release loads them,
/// and runs none yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backends {
    pub collision: CollisionBackend,
    pub pathfinding: PathfindingBackend,
    pub visibility: VisibilityBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionBackend {
    Circles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathfindingBackend {
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityBackend {
    GridFog,
}
