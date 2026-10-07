use crate::units::block::Block;
use crate::units::tag::Tag;
use crate::units::tag_properties::TagProperties;

/// A tag the engine gives and reads, at the same place in every match's tags: packages name it
/// in filters and in the mode's `[tags]`, but no unit type or modifier of theirs carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineTag {
    /// Of avatars, which `unit.is_avatar` tests.
    Avatar,
    /// Of projectile types: a filter selects their units only when it names it.
    Projectile,
    /// Of area types: a filter selects their units only when it names it.
    Area,
    /// Of a site, a building under construction: it blocks its attacks, casts and uses, so no
    /// action of its starts.
    Constructing,
    /// Of node types, which workers gather from.
    Node,
    /// Of drop-off types, which workers take their loads to.
    DropOff,
    /// Of a worker in its gather loop, which another such worker passes through.
    Gathering,
}

impl EngineTag {
    /// In the order of their places, the first of the match's tags.
    pub const ALL: [EngineTag; 7] = [
        EngineTag::Avatar,
        EngineTag::Projectile,
        EngineTag::Area,
        EngineTag::Constructing,
        EngineTag::Node,
        EngineTag::DropOff,
        EngineTag::Gathering,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            EngineTag::Avatar => "avatar",
            EngineTag::Projectile => "projectile",
            EngineTag::Area => "area",
            EngineTag::Constructing => "constructing",
            EngineTag::Node => "node",
            EngineTag::DropOff => "drop_off",
            EngineTag::Gathering => "gathering",
        }
    }

    /// What it does, beside what a filter reads of it.
    pub(crate) const fn properties(self) -> TagProperties {
        match self {
            EngineTag::Avatar
            | EngineTag::Projectile
            | EngineTag::Area
            | EngineTag::Node
            | EngineTag::DropOff
            | EngineTag::Gathering => TagProperties::NONE,
            EngineTag::Constructing => TagProperties::NONE
                .with_block(Block::Attack)
                .with_block(Block::Cast)
                .with_block(Block::Use),
        }
    }

    /// The engine tag `name`, if it is one.
    pub fn named(name: &str) -> Option<EngineTag> {
        EngineTag::ALL.into_iter().find(|tag| tag.name() == name)
    }

    pub(crate) fn tag(self) -> Tag {
        Tag::new(self as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_starts_no_action_and_the_other_engine_tags_block_nothing() {
        let blocks = |tag: EngineTag| Block::ALL.map(|block| tag.properties().blocks(block));
        let (attack, cast, used) = (1, 2, 3);
        let mut site = [false; Block::ALL.len()];
        for at in [attack, cast, used] {
            site[at] = true;
        }
        assert_eq!(Block::ALL[attack], Block::Attack);
        assert_eq!(Block::ALL[cast], Block::Cast);
        assert_eq!(Block::ALL[used], Block::Use);
        assert_eq!(blocks(EngineTag::Constructing), site);
        let others = EngineTag::ALL
            .into_iter()
            .filter(|&tag| tag != EngineTag::Constructing);
        for tag in others {
            assert_eq!(blocks(tag), [false; Block::ALL.len()], "{tag:?}");
        }
    }
}
