use serde::{Deserialize, Serialize};

use crate::bits256::Bits256;
use crate::tag::Tag;

/// A set of tags, one bit each: a unit type's, a modifier's, or a unit's whole set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagSet(Bits256);

impl FromIterator<Tag> for TagSet {
    fn from_iter<I: IntoIterator<Item = Tag>>(tags: I) -> Self {
        tags.into_iter().fold(Self::NONE, Self::with)
    }
}

impl<const N: usize> From<[Tag; N]> for TagSet {
    fn from(tags: [Tag; N]) -> Self {
        tags.into_iter().collect()
    }
}

impl TagSet {
    pub const NONE: Self = Self(Bits256::NONE);

    #[must_use]
    pub const fn with(self, tag: Tag) -> Self {
        Self(self.0.with(tag.get() as usize))
    }

    #[must_use]
    pub const fn without(self, tag: Tag) -> Self {
        Self(self.0.without(tag.get() as usize))
    }

    pub const fn contains(self, tag: Tag) -> bool {
        self.0.contains(tag.get() as usize)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0.union(other.0))
    }

    /// Whether the two hold a tag in common.
    pub const fn meets(self, other: Self) -> bool {
        self.0.meets(other.0)
    }

    /// Whether it holds every tag of `other`.
    pub const fn covers(self, other: Self) -> bool {
        self.0.covers(other.0)
    }

    /// Its tags, in order.
    pub fn iter(self) -> impl Iterator<Item = Tag> {
        let tag = |at| Tag::new(u8::try_from(at).expect("a set of 256 bits holds u8 places"));
        self.0.iter().map(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_tags_in_every_word() {
        let tags = |indices: &[u8]| {
            indices
                .iter()
                .map(|&index| Tag::new(index))
                .collect::<TagSet>()
        };
        let set = tags(&[0, 63, 64, 200, 255]);
        let held: Vec<_> = set.iter().map(Tag::get).collect();
        assert_eq!(held, [0, 63, 64, 200, 255]);
        assert!(set.contains(Tag::new(200)) && !set.contains(Tag::new(199)));
        assert!(set.meets(tags(&[255])) && !set.meets(tags(&[1, 128])));
        assert!(set.covers(tags(&[63, 255])) && !set.covers(tags(&[63, 254])));
        assert!(set.covers(TagSet::NONE));
        assert_eq!(tags(&[1]).union(tags(&[130])), tags(&[1, 130]));
        let cleared = set.without(Tag::new(64)).without(Tag::new(65));
        assert_eq!(cleared, tags(&[0, 63, 200, 255]));
    }
}
