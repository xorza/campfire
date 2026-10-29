use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import converts an object's `BuildVariations` otherwise than whole, or a map's
/// placement of a variation otherwise than to its parent
/// (09-zero-hour.md, A variation varies the body and the look alone;
/// 09-zero-hour.md, A chassis converts to the parent of its file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum VariationDropCause {
    /// A variation names no object: the type holds the others.
    NoObject,
    /// More variations than a type holds: the type holds the first 256.
    TooMany,
    /// A variation differs from the first in more than its body and its look: the type holds the
    /// first alone.
    Unalike,
    /// A variation that several parents name, none of them defined in its own file: a map's
    /// placement of it stays a type of its own.
    Shared,
}
