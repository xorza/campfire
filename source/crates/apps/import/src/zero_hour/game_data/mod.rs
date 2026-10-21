use crate::zero_hour::format::ini::game_data_ini::GameDataIni;

/// What the terrain's conversion reads of the game's `GameData.ini`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GameData {
    /// `AdjustCliffTextures`: a steep cell's tile is stretched to its slope, by its cliff UVs or
    /// by its heights.
    pub(crate) adjust_cliff_textures: bool,
    /// `Use3WayTerrainBlends`: a cell draws its third blend.
    pub(crate) three_way_blends: bool,
}

impl GameData {
    /// What `data` gives the terrain's conversion: a cell draws its third blend for any
    /// `Use3WayTerrainBlends` but 0, as the game tests it.
    pub(crate) const fn of(data: &GameDataIni) -> Self {
        Self {
            adjust_cliff_textures: data.adjust_cliff_textures,
            three_way_blends: data.three_way_blends != 0,
        }
    }
}

#[cfg(test)]
mod tests;
