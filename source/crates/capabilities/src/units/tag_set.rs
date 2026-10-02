use serde::{Deserialize, Serialize};

use crate::units::bits256::Bits256;
use crate::units::tag::Tag;

/// A set of tags, one bit each: a unit type's, a modifier's, or a unit's whole set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct TagSet(Bits256);

impl TagSet {
    pub(crate) fn of(tags: impl IntoIterator<Item = Tag>) -> TagSet {
        tags.into_iter().fold(TagSet::default(), TagSet::with)
    }

    pub(crate) const fn with(self, tag: Tag) -> TagSet {
        TagSet(self.0.with(tag.index()))
    }

    pub(crate) const fn contains(self, tag: Tag) -> bool {
        self.0.contains(tag.index())
    }

    pub(crate) const fn union(self, other: TagSet) -> TagSet {
        TagSet(self.0.union(other.0))
    }

    /// Whether the two hold a tag in common.
    pub(crate) const fn meets(self, other: TagSet) -> bool {
        self.0.meets(other.0)
    }

    /// Whether it holds every tag of `other`.
    pub(crate) const fn covers(self, other: TagSet) -> bool {
        self.0.covers(other.0)
    }

    /// Its tags, in order.
    pub(crate) fn iter(self) -> impl Iterator<Item = Tag> {
        self.0.iter().map(Tag::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_tags_in_every_word() {
        let tags = |indices: &[usize]| TagSet::of(indices.iter().map(|&index| Tag::new(index)));
        let set = tags(&[0, 63, 64, 200, 255]);
        let held: Vec<_> = set.iter().map(Tag::index).collect();
        assert_eq!(held, [0, 63, 64, 200, 255]);
        assert!(set.contains(Tag::new(200)) && !set.contains(Tag::new(199)));
        assert!(set.meets(tags(&[255])) && !set.meets(tags(&[1, 128])));
        assert!(set.covers(tags(&[63, 255])) && !set.covers(tags(&[63, 254])));
        assert!(set.covers(TagSet::default()));
        assert_eq!(tags(&[1]).union(tags(&[130])), tags(&[1, 130]));
    }
}
