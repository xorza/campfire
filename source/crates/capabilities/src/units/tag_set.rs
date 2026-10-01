use std::iter;

use serde::{Deserialize, Serialize};

use crate::units::tag::Tag;

/// A set of tags, one bit each: a unit type's, a modifier's, or a unit's whole set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TagSet([u64; TagSet::WORDS]);

impl TagSet {
    const WORDS: usize = Tag::LIMIT / 64;

    pub(crate) fn of(tags: impl IntoIterator<Item = Tag>) -> TagSet {
        tags.into_iter().fold(TagSet::default(), TagSet::with)
    }

    pub(crate) const fn with(mut self, tag: Tag) -> TagSet {
        self.0[tag.index() / 64] |= 1 << (tag.index() % 64);
        self
    }

    pub(crate) const fn contains(self, tag: Tag) -> bool {
        self.0[tag.index() / 64] & 1 << (tag.index() % 64) != 0
    }

    pub(crate) const fn union(mut self, other: TagSet) -> TagSet {
        let mut word = 0;
        while word < TagSet::WORDS {
            self.0[word] |= other.0[word];
            word += 1;
        }
        self
    }

    /// Whether the two hold a tag in common.
    pub(crate) const fn meets(self, other: TagSet) -> bool {
        let mut word = 0;
        while word < TagSet::WORDS {
            if self.0[word] & other.0[word] != 0 {
                return true;
            }
            word += 1;
        }
        false
    }

    /// Whether it holds every tag of `other`.
    pub(crate) const fn covers(self, other: TagSet) -> bool {
        let mut word = 0;
        while word < TagSet::WORDS {
            if other.0[word] & !self.0[word] != 0 {
                return false;
            }
            word += 1;
        }
        true
    }

    /// Its tags, in order.
    pub(crate) fn iter(self) -> impl Iterator<Item = Tag> {
        self.0.into_iter().enumerate().flat_map(|(word, mut bits)| {
            iter::from_fn(move || {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits.wrapping_sub(1);
                (bit < 64).then(|| Tag::new(word * 64 + bit))
            })
        })
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
