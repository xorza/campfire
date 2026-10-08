use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::units::tag::Tag;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// What each tag of a match does, and each unit type's own tags. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct TagBook {
    /// By tag.
    properties: Vec<TagProperties>,
    /// The tags each tag makes its unit immune to, by tag.
    immune: Vec<TagSet>,
    /// The tags that make their unit immune to some: a modifier that grants one is never
    /// suppressed, so no order of modifiers changes what is.
    granting: TagSet,
    /// Each unit type's own tags, by type.
    type_tags: Vec<TagSet>,
}

impl TagBook {
    /// The book of `tags`, each tag's properties and immunities by its index, and of each unit
    /// type's own tags.
    pub(crate) fn new(
        tags: impl IntoIterator<Item = (TagProperties, TagSet)>,
        types: impl IntoIterator<Item = (UnitType, TagSet)>,
    ) -> TagBook {
        let mut book = TagBook::default();
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
    pub(crate) fn own(&self, unit_type: UnitType) -> TagSet {
        self.type_tags
            .get(unit_type.index())
            .copied()
            .unwrap_or_default()
    }

    /// The tags of a unit of `unit_type` whose modifiers grant `granted`, each set one held
    /// instance's: first the immunities of its type and of the modifiers that grant one, then
    /// every set they do not suppress.
    pub(crate) fn unit_tags(
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
        let takes_effect = TagBook::effect_test(self.granting, immune);
        let tags = granted
            .filter(|&tags| takes_effect(tags))
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
    pub(crate) const fn granting(&self) -> TagSet {
        self.granting
    }

    /// Whether a modifier that grants a set of tags takes effect on a unit immune to `immune`,
    /// where `granting` makes immune: unless it grants a tag the unit is immune to, and none
    /// that makes immune.
    pub(crate) const fn effect_test(granting: TagSet, immune: TagSet) -> impl Fn(TagSet) -> bool {
        move |tags| !tags.meets(immune) || tags.meets(granting)
    }

    /// Whether a modifier that grants a set of tags takes effect on `entity`; every one does in
    /// a match with no tag book, or on a unit with no tags.
    pub(crate) fn effective(world: &World, entity: Entity) -> impl Fn(TagSet) -> bool + use<> {
        let granting = world
            .get_resource::<TagBook>()
            .map_or(TagSet::default(), TagBook::granting);
        let immune = world
            .get::<UnitTags>(entity)
            .map_or(TagSet::default(), |tags| tags.immune);
        TagBook::effect_test(granting, immune)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::units::block::Block;
    use crate::units::engine_tag::EngineTag;
    use crate::units::tag_data::TagData;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::units::unit_types::UnitTypes;
    use crate::values::declared_name::DeclaredName;

    #[test]
    fn a_units_tags_take_their_properties_from_the_modes_data() {
        let mut types = UnitTypes::default();
        let names = ["stunned", "slowed", "slow_immune", "true_sight"];
        let [stunned, slowed, slow_immune, sight] = names.map(|name| types.declare(name));
        // The engine's tags hold the first places, and to declare one finds it.
        assert_eq!(
            [stunned, slowed, slow_immune, sight].map(Tag::index),
            [7, 8, 9, 10]
        );
        let engine = EngineTag::ALL.map(|tag| types.declare(tag.name()));
        assert_eq!(engine, EngineTag::ALL.map(EngineTag::tag));
        assert_eq!(engine.map(Tag::index), [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(types.declare("tower").index(), 11);
        let tower = UnitTypeData {
            tags: vec![DeclaredName::new("true_sight").unwrap()],
            params: BTreeMap::new(),
            state: BTreeMap::new(),
        };
        let tower = types.load(TypeScope::Mode, "tower", &tower);
        let data = |blocks: &[Block], detects, immune: &[&str]| TagData {
            blocks: blocks.to_vec(),
            hidden: false,
            detects,
            immune: immune
                .iter()
                .map(|&name| DeclaredName::new(name).unwrap())
                .collect(),
        };
        let stun = [Block::Move, Block::Attack, Block::Cast, Block::Use];
        let data = BTreeMap::from(
            [
                ("stunned", data(&stun, false, &[])),
                ("slow_immune", data(&[], false, &["slowed"])),
                ("true_sight", data(&[], true, &[])),
            ]
            .map(|(name, data)| (DeclaredName::new(name).unwrap(), data)),
        );
        let book = types.tag_book(&data);
        let set = |tags: &[Tag]| TagSet::of(tags.iter().copied());

        // Its type's true sight detects; a stun blocks what the data names, and no more.
        let tags = book.unit_tags(tower, [set(&[stunned]), set(&[slowed])].into_iter());
        assert_eq!(tags.tags, set(&[stunned, slowed, sight]));
        assert!(tags.properties.detects() && !tags.properties.hidden());
        let blocked: Vec<_> = Block::ALL
            .into_iter()
            .filter(|&block| tags.properties.blocks(block))
            .collect();
        assert_eq!(blocked, stun);
        assert_eq!(tags.immune, TagSet::default());

        // Slow immunity holds a slow without its tag, in either order.
        for granted in [
            [set(&[slowed]), set(&[slow_immune])],
            [set(&[slow_immune]), set(&[slowed])],
        ] {
            let tags = book.unit_tags(tower, granted.into_iter());
            assert_eq!(tags.tags, set(&[sight, slow_immune]));
            assert_eq!(tags.immune, set(&[slowed]));
        }
    }
}
