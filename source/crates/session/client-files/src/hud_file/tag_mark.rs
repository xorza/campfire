use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::{Deserialize, Serialize};

/// What marks a unit that holds the tag `tag` (power.md, A tag marks its unit): the color `tint`
/// adds to its models' light, faded in as the unit gains the tag and out as it loses it, each over
/// `fade_ms`; and whether it stills the clips of its draws that keep no animation of their own,
/// as Zero Hour's `Drawable` tints a disabled object and pauses its animations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagMark {
    pub tag: DeclaredName,
    pub tint: [Num; 3],
    pub fade_ms: u32,
    #[serde(default)]
    pub stills: bool,
}
