use crate::units::tag_set::TagSet;

/// Which modifiers take effect on one unit, by the tags each grants: one takes effect unless it
/// grants a tag the unit is immune to, and none of `granting`, the tags that make immune.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TagEffect {
    granting: TagSet,
    immune: TagSet,
}

impl TagEffect {
    /// The test on a unit immune to `immune`, where `granting` makes immune.
    pub(crate) const fn new(granting: TagSet, immune: TagSet) -> TagEffect {
        TagEffect { granting, immune }
    }

    /// Whether a modifier that grants `tags` takes effect on the unit.
    pub(crate) const fn takes(self, tags: TagSet) -> bool {
        !tags.meets(self.immune) || tags.meets(self.granting)
    }
}
