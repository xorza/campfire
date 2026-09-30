use std::collections::BTreeMap;

use serde::Deserialize;

use crate::units::tag_set::Tag;
use crate::values::scalar::Scalar;

/// A unit type's core fields as its data file declares them: its tags, which filters select, and
/// the params its scripts read as `unit.params`. Each capability the type uses reads its own
/// section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UnitTypeData {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Scalar>,
}

impl UnitTypeData {
    /// The most tags the unit types of a match declare together.
    pub const TAG_LIMIT: usize = Tag::LIMIT;
    /// The most unit types a match loads.
    pub const TYPE_LIMIT: usize = 1 << u16::BITS;

    /// The tag of avatars, which `unit.is_avatar` tests: every avatar carries it.
    pub const AVATAR_TAG: &str = "avatar";
}
