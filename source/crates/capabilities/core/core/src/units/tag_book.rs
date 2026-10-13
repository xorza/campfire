use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::units::tag::Tag;
use crate::units::tag_effect::TagEffect;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::view::View;

/// What each tag of a match does, and each unit type's own tags. Package data, not state.
#[derive(Resource, Debug, Default)]
pub struct TagBook {
    /// By tag.
    properties: Vec<TagProperties>,
    /// The tags each tag makes its unit immune to, by tag.
    immune: Vec<TagSet>,
    /// The tags that make their unit immune to some: a modifier that grants one is never
    /// suppressed, so no order of modifiers changes what is.
    granting: TagSet,
    /// Each unit type's own tags, by type.
    type_tags: Vec<TagSet>,
    /// How many of the first tags are the engine's.
    engine_count: usize,
}

impl TagBook {
    /// The book of `tags`, each tag's properties and immunities by its index, the first
    /// `engine_count` the engine's, and of each unit type's own tags.
    pub fn new(
        engine_count: usize,
        tags: impl IntoIterator<Item = (TagProperties, TagSet)>,
        types: impl IntoIterator<Item = (UnitType, TagSet)>,
    ) -> TagBook {
        let mut book = TagBook {
            engine_count,
            ..TagBook::default()
        };
        for (properties, immune) in tags {
            if immune != TagSet::default() {
                let at = book.properties.len();
                let tag = Tag::new(u8::try_from(at).expect("tags fit u8"));
                book.granting = book.granting.with(tag);
            }
            book.properties.push(properties);
            book.immune.push(immune);
        }
        for (unit_type, tags) in types {
            let at = unit_type.index();
            if book.type_tags.len() <= at {
                book.type_tags.resize(at + 1, TagSet::default());
            }
            book.type_tags[at] = tags;
        }
        book
    }

    /// The own tags of `unit_type`, those of its data and of its sections; none for a type the
    /// book was not given.
    /// How many of the first tags are the engine's.
    pub(crate) const fn engine_count(&self) -> usize {
        self.engine_count
    }

    pub fn own(&self, unit_type: UnitType) -> TagSet {
        self.type_tags
            .get(unit_type.index())
            .copied()
            .unwrap_or_default()
    }

    /// The tags of a unit of `unit_type` whose modifiers grant `granted`, each set one held
    /// instance's: first the immunities of its type and of the modifiers that grant one, then
    /// every set they do not suppress.
    pub fn unit_tags(
        &self,
        unit_type: UnitType,
        granted: impl Iterator<Item = TagSet> + Clone,
    ) -> UnitTags {
        let own = self.own(unit_type);
        let granting = granted.clone().filter(|tags| tags.meets(self.granting));
        let immune = granting
            .fold(own, TagSet::union)
            .iter()
            .filter_map(|tag| self.immune.get(tag.index()))
            .fold(TagSet::default(), |immune, &of| immune.union(of));
        let takes_effect = TagEffect::new(self.granting, immune);
        let tags = granted
            .filter(|&tags| takes_effect.takes(tags))
            .fold(own, TagSet::union);
        let properties = tags
            .iter()
            .filter_map(|tag| self.properties.get(tag.index()))
            .fold(TagProperties::default(), |properties, &of| {
                properties.union(of)
            });
        UnitTags {
            tags,
            properties,
            immune,
        }
    }

    /// The tags that make their unit immune to some.
    pub const fn granting(&self) -> TagSet {
        self.granting
    }
}

impl Book for TagBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::units::tag_book::TagBook;

    impl TagBook {
        /// Whether it holds no tag, as before a match's unit types seal.
        pub(crate) const fn is_empty(&self) -> bool {
            self.properties.is_empty()
        }
    }
}
