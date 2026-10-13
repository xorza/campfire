use std::collections::BTreeMap;
use std::sync::Arc;

use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::units::body_form::BodyForm;
use crate::units::engine_tag::EngineTag;
use crate::units::error::TypesError;
use crate::units::layer::Layer;
use crate::units::tag::Tag;
use crate::units::tag_book::TagBook;
use crate::units::tag_data::TagData;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::type_origin_book::{TypeOrigin, TypeOriginBook};
use crate::units::type_scope::TypeScope;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_core::UnitTypeCore;
use crate::values::declared_name::DeclaredName;
use crate::values::error::NamesError;
use crate::values::name_table::NameTable;
use crate::values::names::Names;
use crate::values::scalar::Scalar;

/// The unit types a match loaded: their names, their tags, their params and their state fields.
/// Package data, not state: a restore loads it from the packages, as a new match does.
#[derive(Debug)]
pub(crate) struct UnitTypes {
    /// The name of each tag, by tag: the engine's, then the match's.
    tags: Names<Tag>,
    /// The engine's tags, the first of `tags`.
    engine: Vec<EngineTag>,
    /// Each type's own tags, by type, until the tag book takes them: from then on the book alone
    /// holds them, and a type loaded or tagged later is a bug.
    staged_tags: Option<Vec<TagSet>>,
    /// The name of each type in its scope, by type; shared with the script view.
    types: Arc<Names<UnitType>>,
    /// Each type's package and its name there, by type.
    origins: TypeOriginBook,
    /// Each type's params, one run per type, in the order of the types.
    params: NameTable<Scalar>,
    /// Each type's script state fields, one run per type, in the order of the types.
    states: NameTable<SyncedStateDecl>,
}

/// No tag and no type: the names a view holds before a match loads its own.
impl Default for UnitTypes {
    fn default() -> UnitTypes {
        UnitTypes::new(Vec::new())
    }
}

impl UnitTypes {
    /// The engine's tags `engine` at their places, as the release lists them, and no type.
    pub(crate) fn new(engine: impl IntoIterator<Item = EngineTag>) -> UnitTypes {
        let engine: Vec<EngineTag> = engine.into_iter().collect();
        UnitTypes {
            tags: engine.iter().map(|tag| tag.name()).collect(),
            engine,
            staged_tags: Some(Vec::new()),
            types: Arc::default(),
            origins: TypeOriginBook::default(),
            params: NameTable::default(),
            states: NameTable::default(),
        }
    }

    /// The engine's tags that a filter selects only when it names one.
    pub(crate) fn named_only(&self) -> TagSet {
        let named_only = self
            .engine
            .iter()
            .filter(|tag| tag.properties().named_only());
        named_only.map(|tag| tag.tag()).collect()
    }

    /// Loads `data` as the type `name` of `scope`, declared by the package at `package`: its tags
    /// join the match's, and its params are kept for `unit.params`; an error for a type its scope
    /// names already, or past the most types or tags a match holds.
    pub(crate) fn load(
        &mut self,
        scope: TypeScope,
        package: u16,
        name: &str,
        data: &UnitTypeCore,
    ) -> Result<UnitType, TypesError> {
        let mut tags = TagSet::default();
        for name in &data.tags {
            tags = tags.with(self.declare(name.as_str())?);
        }
        let params = data
            .params
            .iter()
            .map(|(name, &value)| (name.as_str(), value));
        let added = Arc::make_mut(&mut self.types).try_add((scope, name));
        let unit_type = added.map_err(|error| match error {
            NamesError::Full => TypesError::TooManyTypes,
            NamesError::Repeated => TypesError::Repeated(name.to_owned()),
        })?;
        let run = self.params.push(params);
        debug_assert_eq!(run, unit_type.index(), "one run of params per type");
        let fields = data
            .state
            .iter()
            .map(|(name, field)| (name.as_str(), field.clone()));
        let run = self.states.push(fields);
        debug_assert_eq!(run, unit_type.index(), "one run of state fields per type");
        self.staged_mut().push(tags);
        self.origins.push(TypeOrigin {
            package,
            name: name.into(),
        });
        Ok(unit_type)
    }

    /// The tag `name`, which joins the match's tags if it is new; an error past the most tags a
    /// match holds.
    pub(crate) fn declare(&mut self, name: &str) -> Result<Tag, TypesError> {
        if let Some(tag) = self.tags.named(name) {
            return Ok(tag);
        }
        self.tags.try_add(name).map_err(|error| match error {
            NamesError::Full => TypesError::TooManyTags,
            NamesError::Repeated => unreachable!("the match names no tag {name} yet"),
        })
    }

    /// Gives `unit_type` the tag `tag` too, as the engine tags a type by its sections.
    pub(crate) fn give_tag(&mut self, unit_type: UnitType, tag: Tag) {
        let tags = &mut self.staged_mut()[unit_type.index()];
        *tags = tags.with(tag);
    }

    /// The types' own tags, which the tag book has not taken yet.
    const fn staged_mut(&mut self) -> &mut Vec<TagSet> {
        self.staged_tags
            .as_mut()
            .expect("a type's tags change only before the tag book takes them")
    }

    /// Whether the match loaded `unit_type`.
    pub(crate) fn contains(&self, unit_type: UnitType) -> bool {
        self.types.has(unit_type)
    }

    /// The types' names, each in its scope.
    pub(crate) const fn types(&self) -> &Arc<Names<UnitType>> {
        &self.types
    }

    /// The tags' names: the engine's, then the match's.
    pub(crate) const fn tags(&self) -> &Names<Tag> {
        &self.tags
    }

    /// Each type's package and its name there, as the client finds its look.
    pub(crate) fn origins(&self) -> TypeOriginBook {
        self.origins.clone()
    }

    /// Tags each unit type of `bodies`, each by the body it has, if any, with the name of the
    /// layer it moves on, when `layers` names that layer: the first with no body.
    pub(crate) fn tag_layers(
        &mut self,
        layers: &Names<Layer>,
        bodies: impl IntoIterator<Item = (UnitType, Option<BodyForm>)>,
    ) {
        for (unit_type, body) in bodies {
            let layer = body.map_or(Layer::FIRST, BodyForm::layer);
            if let Some(name) = layers.name(layer) {
                let tag = self
                    .tags()
                    .named(name)
                    .expect("the match declared every tag its packages name");
                self.give_tag(unit_type, tag);
            }
        }
    }

    /// The book of the effects `data` gives the tags, by name, beside an engine tag's own, and of
    /// the types' own tags, which it takes, once: from then on the book alone holds them, and a
    /// later load or tag of a type fails. A tag neither names has none.
    pub(crate) fn tag_book(&mut self, data: &BTreeMap<DeclaredName, TagData>) -> TagBook {
        let tags = self.tags.iter().map(|(tag, name)| {
            let engine = self
                .engine
                .get(tag.index())
                .map_or(TagProperties::NONE, |tag| tag.properties());
            let Some(data) = data.get(name) else {
                return (engine, TagSet::default());
            };
            let immune = data.immune.iter().map(|name| {
                self.tags
                    .named(name.as_str())
                    .expect("the match declared every tag the mode names")
            });
            (engine.union(TagProperties::of(data)), immune.collect())
        });
        let effects: Vec<_> = tags.collect();
        let staged = self
            .staged_tags
            .take()
            .expect("the tag book takes the types' tags once");
        let types = staged.into_iter().enumerate().map(|(at, tags)| {
            (
                UnitType::new(u16::try_from(at).expect("types fit u16")),
                tags,
            )
        });
        TagBook::new(self.engine.len(), effects, types)
    }

    /// The state fields of every type, for the match's book of them.
    pub(crate) fn state_book(&self) -> UnitStateBook {
        UnitStateBook::new(self.states.clone())
    }

    /// The param `name` of `unit_type`, if it declares one.
    pub(crate) fn param(&self, unit_type: UnitType, name: &str) -> Option<Scalar> {
        let run = unit_type.index();
        let at = self.params.named(run, name)?;
        Some(self.params.values(run)[at])
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

#[cfg(test)]
mod tests {
    use crate::release::Release;
    use crate::units::error::TypesError;
    use crate::units::tag::Tag;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type_core::UnitTypeCore;
    use crate::units::unit_types::UnitTypes;
    use crate::values::declared_name::DeclaredName;

    #[test]
    fn the_types_load_up_to_the_most_tags_and_types_a_match_holds() {
        let mut types = UnitTypes::new(Release::engine_tags());
        // The engine's 7 tags and 249 more fill the 256 a match holds.
        for at in Release::engine_tags().len()..Tag::LIMIT {
            types.declare(&format!("tag{at}")).unwrap();
        }
        assert_eq!(types.declare("one_more"), Err(TypesError::TooManyTags));
        // A full match still finds the tags it holds, and a type that names a new one fails.
        assert_eq!(types.declare("tag255").unwrap().index(), 255);
        let named = |name: &str| UnitTypeCore {
            tags: vec![DeclaredName::new(name).unwrap()],
            ..UnitTypeCore::default()
        };
        let load = |types: &mut UnitTypes, name: &str, data: &UnitTypeCore| {
            types.load(TypeScope::Mode, 0, name, data)
        };
        assert_eq!(
            load(&mut types, "tower", &named("one_more")),
            Err(TypesError::TooManyTags)
        );
        // 65,536 types fill a match, and one more fails. The names sort in the order they load.
        let plain = UnitTypeCore::default();
        for at in 0..UnitTypeCore::TYPE_LIMIT {
            load(&mut types, &format!("type{at:05}"), &plain).unwrap();
        }
        assert_eq!(
            load(&mut types, "type65536", &named("tag255")),
            Err(TypesError::TooManyTypes)
        );
    }
}
