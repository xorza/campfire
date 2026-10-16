use crate::block::Block;

/// The properties of a set of tags: what they block, whether they hide their unit or let it detect
/// hidden units, and two that only engine tags hold: whether a filter selects their unit only when
/// it names one of them, and whether their unit passes through each unit that holds them too.
/// Each system asks it the one question it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TagProperties(u16);

impl TagProperties {
    /// No property.
    pub const NONE: Self = Self(0);
    const HIDDEN: u16 = 1 << Block::ALL.len();
    const DETECTS: u16 = Self::HIDDEN << 1;
    const NAMED_ONLY: u16 = Self::DETECTS << 1;
    const PASSES: u16 = Self::NAMED_ONLY << 1;

    #[must_use]
    pub const fn with_block(self, block: Block) -> Self {
        Self(self.0 | (1 << block as u8))
    }

    #[must_use]
    pub const fn with_hidden(self) -> Self {
        Self(self.0 | Self::HIDDEN)
    }

    #[must_use]
    pub const fn with_detects(self) -> Self {
        Self(self.0 | Self::DETECTS)
    }

    /// The property of a delivery's tag: a filter selects its unit only when it names it.
    #[must_use]
    pub const fn with_named_only(self) -> Self {
        Self(self.0 | Self::NAMED_ONLY)
    }

    /// The property of a gathering worker's tag: its unit passes through each unit that holds it
    /// too.
    #[must_use]
    pub const fn with_passes(self) -> Self {
        Self(self.0 | Self::PASSES)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn blocks(self, block: Block) -> bool {
        self.0 & (1 << block as u8) != 0
    }

    pub const fn hidden(self) -> bool {
        self.0 & Self::HIDDEN != 0
    }

    pub const fn detects(self) -> bool {
        self.0 & Self::DETECTS != 0
    }

    pub const fn named_only(self) -> bool {
        self.0 & Self::NAMED_ONLY != 0
    }

    pub const fn passes(self) -> bool {
        self.0 & Self::PASSES != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_property_is_its_own_bit() {
        let none = TagProperties::default();
        for block in Block::ALL {
            let properties = none.with_block(block);
            let blocked: Vec<_> = Block::ALL
                .into_iter()
                .filter(|&b| properties.blocks(b))
                .collect();
            assert_eq!(blocked, [block]);
            assert!(!properties.hidden() && !properties.detects());
        }
        let both = none.with_hidden().union(none.with_detects());
        assert!(both.hidden() && both.detects());
        assert!(Block::ALL.into_iter().all(|block| !both.blocks(block)));
    }
}
