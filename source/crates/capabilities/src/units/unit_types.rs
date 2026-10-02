use std::collections::BTreeMap;

use crate::units::engine_tag::EngineTag;
use crate::units::tag::Tag;
use crate::units::tag_book::TagBook;
use crate::units::tag_data::TagData;
use crate::units::tag_effects::TagEffects;
use crate::units::tag_set::TagSet;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::name_table::NameTable;
use crate::values::scalar::Scalar;

/// The unit types a match loaded: their names, their tags and their params. Package data, not
/// state: a restore loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct UnitTypes {
    /// The name of each tag, by tag: the engine's, then the match's.
    tag_names: Vec<Box<str>>,
    types: Vec<TypeEntry>,
    /// Every type, sorted by scope, then name.
    by_name: Vec<UnitType>,
    /// Each type's params, one run per type, in the order of the types.
    params: NameTable<Scalar>,
}

/// A loaded unit type: the scope its name is seen in, its name, and its tags.
#[derive(Debug)]
struct TypeEntry {
    scope: TypeScope,
    name: Box<str>,
    tags: TagSet,
}

/// The engine's tags at their places, and no type.
impl Default for UnitTypes {
    fn default() -> UnitTypes {
        UnitTypes {
            tag_names: EngineTag::ALL.map(|tag| tag.name().into()).into(),
            types: Vec::new(),
            by_name: Vec::new(),
            params: NameTable::default(),
        }
    }
}

impl UnitTypes {
    /// Loads `data` as the type `name` of `scope`, which the package load checked names no other
    /// type there, within the most types a match loads: its tags join the match's, and its params
    /// are kept for `unit.params`.
    pub(crate) fn load(&mut self, scope: TypeScope, name: &str, data: &UnitTypeData) -> UnitType {
        let index =
            u16::try_from(self.types.len()).expect("the load keeps the unit types within u16");
        let Err(at) = self.find(scope, name) else {
            panic!("a scope names a type once, which the load checked: {name}");
        };
        let mut tags = TagSet::default();
        for name in &data.tags {
            tags = tags.with(self.declare(name.as_str()));
        }
        let params = data
            .params
            .iter()
            .map(|(name, &value)| (name.as_str(), value));
        let run = self.params.push(params);
        debug_assert_eq!(run, usize::from(index), "one run of params per type");
        self.by_name.insert(at, UnitType::new(index));
        self.types.push(TypeEntry {
            scope,
            name: name.into(),
            tags,
        });
        UnitType::new(index)
    }

    /// The tag `name`, which joins the match's tags if it is new, within the most tags a match
    /// has, which the package load counted.
    pub(crate) fn declare(&mut self, name: &str) -> Tag {
        if let Some(tag) = self.tag(name) {
            return tag;
        }
        assert!(
            self.tag_names.len() < Tag::LIMIT,
            "the load counted the tags"
        );
        self.tag_names.push(name.into());
        Tag::new(self.tag_names.len() - 1)
    }

    /// Gives `unit_type` the tag `tag` too, as the engine tags a type by its sections.
    pub(crate) fn give_tag(&mut self, unit_type: UnitType, tag: Tag) {
        let entry = &mut self.types[unit_type.index()];
        entry.tags = entry.tags.with(tag);
    }

    /// The type named `name` in `scope`.
    pub(crate) fn named(&self, scope: TypeScope, name: &str) -> Option<UnitType> {
        Some(self.by_name[self.find(scope, name).ok()?])
    }

    /// Where `name` of `scope` is in `by_name`, or where it would go.
    fn find(&self, scope: TypeScope, name: &str) -> Result<usize, usize> {
        self.by_name.binary_search_by(|&unit_type| {
            let entry = &self.types[unit_type.index()];
            entry
                .scope
                .cmp(&scope)
                .then_with(|| (*entry.name).cmp(name))
        })
    }

    pub(crate) fn name(&self, unit_type: UnitType) -> &str {
        &self.types[unit_type.index()].name
    }

    /// The tag `name`, once declared.
    pub(crate) fn tag(&self, name: &str) -> Option<Tag> {
        let index = self.tag_names.iter().position(|tag| **tag == *name)?;
        Some(Tag::new(index))
    }

    /// The book of the effects `data` gives the tags, by name, and of the types' own tags. A
    /// tag `data` does not name has none.
    pub(crate) fn tag_book(&self, data: &BTreeMap<DeclaredName, TagData>) -> TagBook {
        let tags = self.tag_names.iter().map(|name| {
            let Some(data) = data.get(&**name) else {
                return (TagEffects::default(), TagSet::default());
            };
            let immune = data.immune.iter().map(|name| {
                self.tag(name.as_str())
                    .expect("the match declared every tag the mode names")
            });
            (TagEffects::of(data), TagSet::of(immune))
        });
        let types = self.types.iter().enumerate().map(|(at, entry)| {
            (
                UnitType::new(u16::try_from(at).expect("types fit u16")),
                entry.tags,
            )
        });
        TagBook::new(tags, types)
    }

    /// The param `name` of `unit_type`, if it declares one.
    pub(crate) fn param(&self, unit_type: UnitType, name: &str) -> Option<Scalar> {
        self.params.get(unit_type.index(), name).copied()
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;

    impl UnitTypes {
        pub(crate) fn count(&self) -> usize {
            self.types.len()
        }

        pub(crate) fn tags(&self, unit_type: UnitType) -> TagSet {
            self.types[unit_type.index()].tags
        }
    }
}
