use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why a map's road segments are not drawn (09-zero-hour.md, A road's type is `Roads.ini`'s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum RoadDropCause {
    /// Segments of a template no road type names, by their template, where the type of id 1 the
    /// game gives them is a bridge or none, which draw nothing.
    NoType,
    /// Segments of a type whose texture no archive holds, which draw nothing.
    NoTexture,
    /// Segments of a type whose texture finds no room in the terrain's texture, or whose sides
    /// its levels do not halve to whole texels, which draw nothing.
    NoPlace,
}
