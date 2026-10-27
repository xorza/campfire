use campfire_values::{EngineTag, TagProperties};

use crate::status_tags::StatusTags;

/// The status of a unit that flies on wings in the air, which never stands and aims by flying at
/// its target: a contract the capability that flies it turns, and those that aim and chase with
/// it read (jets.md, A jet flies on wings; jets.md, A jet aims by flying).
#[derive(Debug)]
pub struct OnWings;

impl OnWings {
    /// The engine tag of a unit on wings in the air.
    pub const TAG: EngineTag = EngineTag::new(3, "on_wings", TagProperties::NONE);

    /// Whether the unit of `status` flies on wings in the air.
    pub fn holds(status: Option<&StatusTags>) -> bool {
        status.is_some_and(|status| status.get().contains(Self::TAG.tag()))
    }
}
