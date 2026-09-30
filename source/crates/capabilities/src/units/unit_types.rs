use crate::units::error::UnitTypeError;
use crate::units::tag_set::{Tag, TagSet};
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
    /// The `hero` tag, which `unit.is_hero` tests, once a type declares it.
    hero: Option<Tag>,
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
        let mut new_tags = 0;
        for (at, name) in data.tags.iter().enumerate() {
            let seen = data.tags[..at].contains(name);
            if !seen && self.tag(name).is_none() {
                new_tags += 1;
            }
        }
        if self.tag_names.len() + new_tags > Tag::LIMIT {
            return Err(UnitTypeError::TooManyTags);
        }
        let mut tags = TagSet::default();
        for name in &data.tags {
            let tag = self.tag(name).unwrap_or_else(|| {
                self.tag_names.push(name.as_str().into());
                Tag::new(self.tag_names.len() - 1)
            });
            if name == UnitTypeData::HERO_TAG {
                self.hero = Some(tag);
            }
            tags = tags.with(tag);
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

    /// The tag `name` of a loaded type.
    pub(crate) fn tag(&self, name: &str) -> Option<Tag> {
        let index = self.tag_names.iter().position(|tag| **tag == *name)?;
        Some(Tag::new(index))
    }

    pub(crate) fn tags(&self, unit_type: UnitType) -> TagSet {
        self.types[unit_type.index()].tags
    }

    pub(crate) const fn hero(&self) -> Option<Tag> {
        self.hero
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
    }
}
