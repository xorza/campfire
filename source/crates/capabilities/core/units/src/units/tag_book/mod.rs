use std::iter;

use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::DeferredWorld;
use campfire_declare::Book;
use campfire_script::View;
use campfire_sim::Capability;
use campfire_values::{Tag, TagProperties, TagSet, UnitTags, UnitType};

use crate::units::avatar::Avatar;
use crate::units::tag_book::tag_effect::TagEffect;

pub(crate) mod tag_effect;

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
    /// The hook that gives a unit its type's tags as its type is inserted, from the tag book, none
    /// in a world with none, as a copy of a match's state; and marks it an `Avatar` exactly when
    /// they hold `avatar`. Its modifiers' grants join them as stats derive them. A type the match
    /// did not load has no tags, and the restore check refuses it.
    pub(crate) fn give_tags(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit_type = *world
            .get::<UnitType>(entity)
            .expect("the hook's own component");
        let tags = world
            .get_resource::<TagBook>()
            .map_or_else(UnitTags::default, |book| {
                book.unit_tags(unit_type, iter::empty())
            });
        let avatar = tags.tags.contains(Avatar::TAG.tag());
        *world
            .get_mut::<UnitTags>(entity)
            .expect("a unit type requires its tags") = tags;
        let marked = world.entity(entity).contains::<Avatar>();
        if avatar && !marked {
            world.commands().entity(entity).insert(Avatar);
        } else if !avatar && marked {
            world.commands().entity(entity).remove::<Avatar>();
        }
    }

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

    fn share(&self, _: &View) {}
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
