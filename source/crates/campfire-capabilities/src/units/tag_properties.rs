use crate::units::block::Block;
use crate::units::tag_data::TagData;

/// The properties of a set of tags: what they block, and whether they hide their unit or let it
/// detect hidden units. Each system asks it the one question it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TagProperties(u8);

impl TagProperties {
    const HIDDEN: u8 = 1 << Block::ALL.len();
    const DETECTS: u8 = TagProperties::HIDDEN << 1;

    /// The properties `data` gives a tag.
    pub(crate) fn of(data: &TagData) -> TagProperties {
        let blocks = data
            .blocks
            .iter()
            .fold(TagProperties::default(), |properties, &block| {
                properties.with_block(block)
            });
        let hidden = if data.hidden {
            blocks.with_hidden()
        } else {
            blocks
        };
        if data.detects {
            hidden.with_detects()
        } else {
            hidden
        }
    }

    pub(crate) const fn with_block(self, block: Block) -> TagProperties {
        TagProperties(self.0 | 1 << block as u8)
    }

    pub(crate) const fn with_hidden(self) -> TagProperties {
        TagProperties(self.0 | TagProperties::HIDDEN)
    }

    pub(crate) const fn with_detects(self) -> TagProperties {
        TagProperties(self.0 | TagProperties::DETECTS)
    }

    pub(crate) const fn union(self, other: TagProperties) -> TagProperties {
        TagProperties(self.0 | other.0)
    }

    pub(crate) const fn blocks(self, block: Block) -> bool {
        self.0 & 1 << block as u8 != 0
    }

    pub(crate) const fn hidden(self) -> bool {
        self.0 & TagProperties::HIDDEN != 0
    }

    pub(crate) const fn detects(self) -> bool {
        self.0 & TagProperties::DETECTS != 0
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
