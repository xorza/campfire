/// A unit type's tags, one bit each. A match has at most 64 tags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TagSet(u64);

/// A tag, by its place in the match's list of tag names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tag(u8);

impl TagSet {
    pub(crate) const fn with(self, tag: Tag) -> TagSet {
        TagSet(self.0 | 1 << tag.0)
    }

    pub(crate) const fn contains(self, tag: Tag) -> bool {
        self.0 & 1 << tag.0 != 0
    }
}

impl Tag {
    /// The most tags a match has.
    pub(crate) const LIMIT: usize = 64;

    /// The tag at `index` in the list of names, which is below `LIMIT`.
    pub(crate) fn new(index: usize) -> Tag {
        debug_assert!(index < Tag::LIMIT);
        Tag(u8::try_from(index).expect("a tag index is below the limit"))
    }
}
