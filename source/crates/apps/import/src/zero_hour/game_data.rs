/// What the terrain's conversion reads of the game's `GameData.ini`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GameData {
    /// `AdjustCliffTextures`: a steep cell's tile is stretched to its slope, by its cliff UVs or
    /// by its heights.
    pub(crate) adjust_cliff_textures: bool,
    /// `Use3WayTerrainBlends`: a cell draws its third blend.
    pub(crate) three_way_blends: bool,
}
