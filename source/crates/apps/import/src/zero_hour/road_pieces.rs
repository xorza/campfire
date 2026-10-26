use serde::{Deserialize, Serialize};

/// The road pieces a map's import draws, by their kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoadPieces {
    /// Straight segments, each between two ends of the map's.
    pub segments: usize,
    /// Curves, each of 30° or less, where two segments meet.
    pub curves: usize,
    /// Miters, where two segments meet at a turn too sharp or too slight for a curve, or of an
    /// angled corner.
    pub miters: usize,
    /// Tees, where three ends meet, two of them a straight road and the third across it.
    pub tees: usize,
    /// Four-ways, where four ends meet.
    pub four_ways: usize,
    /// Ys, where three ends meet, no two of them near a straight road.
    pub ys: usize,
    /// Hs, tees whose third end meets the straight road slanted, and their mirrors.
    pub hs: usize,
    /// Alpha joins, where a road ends at one of another type.
    pub joins: usize,
}
