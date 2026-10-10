use serde::{Deserialize, Serialize};

/// What the client of an imported Zero Hour package reads of the game's `GameData.ini`,
/// `client/game_data.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameData {
    /// `AdjustCliffTextures`: a steep cell's tile is stretched to its slope, by its cliff UVs or
    /// by its heights.
    pub adjust_cliff_textures: bool,
    /// `Use3WayTerrainBlends`: a cell draws its third blend.
    pub three_way_blends: bool,
}

impl GameData {
    /// Where an imported package holds it.
    pub const PATH: &'static str = "client/game_data.toml";
}
