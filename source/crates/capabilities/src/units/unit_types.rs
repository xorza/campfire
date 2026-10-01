use std::collections::BTreeMap;

use crate::units::error::UnitTypeError;
use crate::units::tag::Tag;
use crate::units::tag_book::TagBook;
use crate::units::tag_data::TagData;
use crate::units::tag_effects::TagEffects;
use crate::units::tag_set::TagSet;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::name_table::NameTable;
use crate::values::scalar::Scalar;

/// The unit types a match loaded: their names, their tags and their params. Package data, not
/// state: a restore loads it from the packages, as a new match does.
#[derive(Debug, Default)]
pub(crate) struct UnitTypes {
    /// The name of each tag, by tag.
    tag_names: Vec<Box<str>>,
    /// The `avatar` tag, which `unit.is_avatar` tests, once declared.
    avatar: Option<Tag>,
    types: Vec<TypeEntry>,
    /// Every type, sorted by name.
    by_name: Vec<UnitType>,
    /// Each type's params, one run per type, in the order of the types.
    params: NameTable<Scalar>,
}

/// A loaded unit type: its name and its tags.
#[derive(Debug)]
struct TypeEntry {
    name: Box<str>,
    tags: TagSet,
}

impl UnitTypes {
    /// Loads `data` as the type `name`: its tags join the match's, and its params are kept for
    /// `unit.params`.
    pub(crate) fn load(
        &mut self,
        name: &str,
        data: &UnitTypeData,
    ) -> Result<UnitType, UnitTypeError> {
        let index = u16::try_from(self.types.len())
            .ok()
            .ok_or(UnitTypeError::TooManyTypes)?;
        let Err(at) = self.find(name) else {
            return Err(UnitTypeError::RepeatedName);
        };
        let mut tags = TagSet::default();
        for name in &data.tags {
            tags = tags.with(self.declare(name)?);
        }
        let params = data
            .params
            .iter()
            .map(|(name, &value)| (name.as_str(), value));
        let run = self.params.push(params);
        debug_assert_eq!(run, usize::from(index), "one run of params per type");
        self.by_name.insert(at, UnitType::new(index));
        self.types.push(TypeEntry {
            name: name.into(),
            tags,
        });
        Ok(UnitType::new(index))
    }

    /// The tag `name`, which joins the match's tags if it is new.
    pub(crate) fn declare(&mut self, name: &str) -> Result<Tag, UnitTypeError> {
        if let Some(tag) = self.tag(name) {
            return Ok(tag);
        }
        if self.tag_names.len() == Tag::LIMIT {
            return Err(UnitTypeError::TooManyTags);
        }
        self.tag_names.push(name.into());
        let tag = Tag::new(self.tag_names.len() - 1);
        if name == UnitTypeData::AVATAR_TAG {
            self.avatar = Some(tag);
        }
        Ok(tag)
    }

    /// The type named `name`.
    pub(crate) fn named(&self, name: &str) -> Option<UnitType> {
        Some(self.by_name[self.find(name).ok()?])
    }

    /// Where `name` is in `by_name`, or where it would go.
    fn find(&self, name: &str) -> Result<usize, usize> {
        self.by_name
            .binary_search_by(|&unit_type| self.name(unit_type).cmp(name))
    }

    pub(crate) fn name(&self, unit_type: UnitType) -> &str {
        &self.types[unit_type.index()].name
    }

    /// The tag `name`, once declared.
    pub(crate) fn tag(&self, name: &str) -> Option<Tag> {
        let index = self.tag_names.iter().position(|tag| **tag == *name)?;
        Some(Tag::new(index))
    }

    pub(crate) const fn avatar(&self) -> Option<Tag> {
        self.avatar
    }

    /// The book of the effects `data` gives the tags, by name, and of the types' own tags. A
    /// tag `data` does not name has none.
    pub(crate) fn tag_book(&self, data: &BTreeMap<String, TagData>) -> TagBook {
        let tags = self.tag_names.iter().map(|name| {
            let Some(data) = data.get(&**name) else {
                return (TagEffects::default(), TagSet::default());
            };
            let immune = data.immune.iter().map(|name| {
                self.tag(name)
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
