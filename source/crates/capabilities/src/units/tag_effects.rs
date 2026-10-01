use crate::units::block::Block;
use crate::units::tag_data::TagData;

/// The effects of a set of tags: what they block, and whether they hide their unit or let it
/// detect hidden units. Each system asks it the one question it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TagEffects(u8);

impl TagEffects {
    const HIDDEN: u8 = 1 << Block::ALL.len();
    const DETECTS: u8 = TagEffects::HIDDEN << 1;

    /// The effects `data` gives a tag.
    pub(crate) fn of(data: &TagData) -> TagEffects {
        let blocks = data
            .blocks
            .iter()
            .fold(TagEffects::default(), |effects, &block| {
                effects.with_block(block)
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

    pub(crate) const fn with_block(self, block: Block) -> TagEffects {
        TagEffects(self.0 | 1 << block as u8)
    }

    pub(crate) const fn with_hidden(self) -> TagEffects {
        TagEffects(self.0 | TagEffects::HIDDEN)
    }

    pub(crate) const fn with_detects(self) -> TagEffects {
        TagEffects(self.0 | TagEffects::DETECTS)
    }

    pub(crate) const fn union(self, other: TagEffects) -> TagEffects {
        TagEffects(self.0 | other.0)
    }

    pub(crate) const fn blocks(self, block: Block) -> bool {
        self.0 & 1 << block as u8 != 0
    }

    pub(crate) const fn hidden(self) -> bool {
        self.0 & TagEffects::HIDDEN != 0
    }

    pub(crate) const fn detects(self) -> bool {
        self.0 & TagEffects::DETECTS != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_effect_is_its_own_bit() {
        let none = TagEffects::default();
        for block in Block::ALL {
            let effects = none.with_block(block);
            let blocked: Vec<_> = Block::ALL
                .into_iter()
                .filter(|&b| effects.blocks(b))
                .collect();
            assert_eq!(blocked, [block]);
            assert!(!effects.hidden() && !effects.detects());
        }
        let both = none.with_hidden().union(none.with_detects());
        assert!(both.hidden() && both.detects());
        assert!(Block::ALL.into_iter().all(|block| !both.blocks(block)));
    }
}
