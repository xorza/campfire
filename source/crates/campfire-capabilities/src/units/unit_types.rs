use std::collections::BTreeMap;
use std::mem;

use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::units::engine_tag::EngineTag;
use crate::units::tag::Tag;
use crate::units::tag_book::TagBook;
use crate::units::tag_data::TagData;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::type_scope::TypeScope;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::name_list::NameList;
use crate::values::name_table::NameTable;
use crate::values::scalar::Scalar;

/// The unit types a match loaded: their names, their tags, their params and their state fields.
/// Package data, not state: a restore loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct UnitTypes {
    /// The name of each tag, by tag: the engine's, then the match's.
    tag_names: NameList,
    types: Vec<TypeEntry>,
    /// The name of each type in its scope, by type.
    type_names: NameList,
    /// Every type, sorted by scope, then name.
    by_name: Vec<UnitType>,
    /// Each type's params, one run per type, in the order of the types.
    params: NameTable<Scalar>,
    /// Each type's script state fields, one run per type, in the order of the types.
    states: NameTable<SyncedStateDecl>,
}

/// A loaded unit type: the scope its name is seen in, and its tags.
#[derive(Debug)]
struct TypeEntry {
    scope: TypeScope,
    tags: TagSet,
}

/// The engine's tags at their places, and no type.
impl Default for UnitTypes {
    fn default() -> UnitTypes {
        UnitTypes {
            tag_names: EngineTag::ALL.into_iter().map(EngineTag::name).collect(),
            types: Vec::new(),
            type_names: NameList::default(),
            by_name: Vec::new(),
            params: NameTable::default(),
            states: NameTable::default(),
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
        let fields = data
            .state
            .iter()
            .map(|(name, field)| (name.as_str(), field.clone()));
        let run = self.states.push(fields);
        debug_assert_eq!(run, usize::from(index), "one run of state fields per type");
        self.by_name.insert(at, UnitType::new(index));
        self.types.push(TypeEntry { scope, tags });
        self.type_names.push(name);
        UnitType::new(index)
    }

    /// The tag `name`, which joins the match's tags if it is new, within the most tags a match
    /// has, which the package load counted.
    pub(crate) fn declare(&mut self, name: &str) -> Tag {
        if let Some(tag) = self.tag_named(name) {
            return tag;
        }
        assert!(
            self.tag_names.len() < Tag::LIMIT,
            "the load counted the tags"
        );
        Tag::new(self.tag_names.push(name))
    }

    /// Gives `unit_type` the tag `tag` too, as the engine tags a type by its sections.
    pub(crate) fn give_tag(&mut self, unit_type: UnitType, tag: Tag) {
        let entry = &mut self.types[unit_type.index()];
        entry.tags = entry.tags.with(tag);
    }

    /// Whether the match loaded `unit_type`.
    pub(crate) const fn contains(&self, unit_type: UnitType) -> bool {
        unit_type.index() < self.types.len()
    }

    /// The type named `name` in `scope`.
    pub(crate) fn named(&self, scope: TypeScope, name: &str) -> Option<UnitType> {
        Some(self.by_name[self.find(scope, name).ok()?])
    }

    /// Where `name` of `scope` is in `by_name`, or where it would go.
    fn find(&self, scope: TypeScope, name: &str) -> Result<usize, usize> {
        self.by_name.binary_search_by(|&unit_type| {
            let index = unit_type.index();
            let held = self.type_names.get(index).expect("a loaded type");
            self.types[index]
                .scope
                .cmp(&scope)
                .then_with(|| held.cmp(name))
        })
    }

    /// Every type's name, by type.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.type_names.iter()
    }

    /// The tag `name`, once declared.
    pub(crate) fn tag_named(&self, name: &str) -> Option<Tag> {
        self.tag_names.named(name).map(Tag::new)
    }

    /// The book of the effects `data` gives the tags, by name, and of the types' own tags, which
    /// it takes: from then on the book alone holds them. A tag `data` does not name has none.
    pub(crate) fn tag_book(&mut self, data: &BTreeMap<DeclaredName, TagData>) -> TagBook {
        let tags = self.tag_names.iter().map(|name| {
            let Some(data) = data.get(name) else {
                return (TagProperties::default(), TagSet::default());
            };
            let immune = data.immune.iter().map(|name| {
                self.tag_named(name.as_str())
                    .expect("the match declared every tag the mode names")
            });
            (TagProperties::of(data), TagSet::of(immune))
        });
        let effects: Vec<_> = tags.collect();
        let types = self.types.iter_mut().enumerate().map(|(at, entry)| {
            (
                UnitType::new(u16::try_from(at).expect("types fit u16")),
                mem::take(&mut entry.tags),
            )
        });
        TagBook::new(effects, types)
    }

    /// The state fields of every type, for the match's book of them.
    pub(crate) fn state_book(&self) -> UnitStateBook {
        UnitStateBook::new(self.states.clone())
    }

    /// The param `name` of `unit_type`, if it declares one.
    pub(crate) fn param_named(&self, unit_type: UnitType, name: &str) -> Option<Scalar> {
        self.params.get_named(unit_type.index(), name).copied()
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
