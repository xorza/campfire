use std::ops::Range;

use crate::units::error::UnitTypeError;
use crate::units::scalar::Scalar;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;

/// The unit types a match loaded: their tags and their params. Package data, not state: a
/// restore loads it from the packages, as a new match does.
#[derive(Debug, Default)]
pub(crate) struct UnitTypes {
    /// The name of each tag, by tag.
    tag_names: Vec<Box<str>>,
    /// The `hero` tag, which `unit.is_hero` tests, once a type declares it.
    hero: Option<Tag>,
    types: Vec<TypeEntry>,
    /// The param names of every type, sorted, one run per type.
    param_names: Vec<Box<str>>,
    /// The param values, in the order of their names.
    param_values: Vec<Scalar>,
}

/// A loaded unit type: its tags, and its run of params.
#[derive(Debug)]
struct TypeEntry {
    tags: TagSet,
    params: Range<u32>,
}

impl UnitTypes {
    /// Loads `data`: its tags join the match's, and its params are kept for `unit.params`.
    pub(crate) fn load(&mut self, data: &UnitTypeData) -> Result<UnitType, UnitTypeError> {
        let index = u16::try_from(self.types.len())
            .ok()
            .ok_or(UnitTypeError::TooManyTypes)?;
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
            if name == "hero" {
                self.hero = Some(tag);
            }
            tags = tags.with(tag);
        }
        let start = u32::try_from(self.param_names.len()).expect("params fit u32");
        self.param_names
            .extend(data.params.keys().map(|name| name.as_str().into()));
        self.param_values.extend(data.params.values().copied());
        let end = u32::try_from(self.param_names.len()).expect("params fit u32");
        self.types.push(TypeEntry {
            tags,
            params: start..end,
        });
        Ok(UnitType::new(index))
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
        let run = &self.types[unit_type.index()].params;
        let (start, end) = (run.start as usize, run.end as usize);
        let at = self.param_names[start..end]
            .binary_search_by(|probe| (**probe).cmp(name))
            .ok()?;
        Some(self.param_values[start + at])
    }
}
