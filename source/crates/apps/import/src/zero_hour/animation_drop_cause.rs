use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops an animation a look names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum AnimationDropCause {
    /// No file of the install holds it: its file, `<animation>.w3d` of its name
    /// `<hierarchy>.<animation>`, is missing or holds no animation of its name.
    Missing,
    /// No file of the install holds the hierarchy it names.
    NoHierarchy,
    /// Its hierarchy has no pivot past its root, which `HTreeClass::Anim_Update` never moves, so
    /// it moves nothing.
    NoPivot,
}
