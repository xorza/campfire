use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_script::{StateSchema, TagNames};
use campfire_values::{
    DeclaredName, EngineTag, NameId, NameTable, Names, NamesError, Scalar, Tag, TagProperties,
    TagSet, TypeScope, UnitType,
};

use crate::body_form::BodyForm;
use crate::error::TypesError;
use crate::layer::Layer;
use crate::tag_book::TagBook;
use crate::tag_data::TagData;
use crate::type_origin_book::{TypeOrigin, TypeOriginBook};
use crate::unit_state_book::UnitStateBook;
use crate::unit_type_book::UnitTypeBook;
use crate::unit_type_core::UnitTypeCore;

/// The unit types a match loads from its packages, and the tags they and the mode name, until
/// `finish` makes the four books of them.
#[derive(Debug)]
pub struct UnitTypes {
    /// The name of each tag, by tag: the engine's, then the match's.
    tags: TagNames,
    /// The engine's tags, the first of `tags`.
    engine: Vec<EngineTag>,
    /// Each type's own tags, by type.
    type_tags: Vec<TagSet>,
    /// The name of each type in its scope, by type.
    types: Arc<Names<UnitType>>,
    /// Each type's package and its name there, by type.
    origins: TypeOriginBook,
    /// Each type's params, one run per type, in the order of the types.
    params: Arc<NameTable<Scalar>>,
    /// Each type's script state, a schema per type, in the order of the types.
    states: Vec<StateSchema>,
}

/// The books of a match's unit types, which `UnitTypes::finish` makes.
#[derive(Debug)]
pub struct UnitTypeBooks {
    pub types: UnitTypeBook,
    pub tags: TagBook,
    pub origins: TypeOriginBook,
    pub states: UnitStateBook,
}

/// No tag and no type.
impl Default for UnitTypes {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl UnitTypes {
    /// The engine's tags `engine` at their places, as the release lists them, and no type.
    pub fn new(engine: impl IntoIterator<Item = EngineTag>) -> Self {
        let engine: Vec<EngineTag> = engine.into_iter().collect();
        Self {
            tags: TagNames::new(&engine),
            engine,
            type_tags: Vec::new(),
            types: Arc::default(),
            origins: TypeOriginBook::default(),
            params: Arc::default(),
            states: Vec::new(),
        }
    }

    /// Loads `data` as the type of `scope` that `origin` names: its tags join the match's, and its
    /// params are kept for `unit.params`; an error for a type its scope names already, or past the
    /// most types or tags a match holds.
    pub fn load(
        &mut self,
        scope: TypeScope,
        origin: TypeOrigin,
        data: &UnitTypeCore,
    ) -> Result<UnitType, TypesError> {
        let mut tags = TagSet::NONE;
        for name in &data.tags {
            tags = tags.with(self.declare(name.as_str())?);
        }
        let params = data
            .params
            .iter()
            .map(|(name, &value)| (name.as_str(), value));
        let added = Arc::make_mut(&mut self.types).try_add((scope, &*origin.name));
        let unit_type = match added {
            Ok(unit_type) => unit_type,
            Err(NamesError::Full) => return Err(TypesError::TooManyTypes),
            Err(NamesError::Repeated) => return Err(TypesError::Repeated(origin.name.into())),
        };
        let run = Arc::make_mut(&mut self.params).push(params);
        debug_assert_eq!(run, unit_type.index(), "one run of params per type");
        let fields = data.state.iter();
        let fields = fields.map(|(name, field)| (name.as_str(), field.sync, &field.decl));
        self.states.push(StateSchema::new(fields));
        debug_assert_eq!(
            self.states.len(),
            unit_type.index() + 1,
            "one schema per type"
        );
        self.type_tags.push(tags);
        self.origins.push(origin);
        Ok(unit_type)
    }

    /// The tag `name`, which joins the match's tags if it is new; an error past the most tags a
    /// match holds.
    pub fn declare(&mut self, name: &str) -> Result<Tag, TypesError> {
        if let Some(tag) = self.tags.named(name) {
            return Ok(tag);
        }
        self.tags
            .names_mut()
            .try_add(name)
            .map_err(|error| match error {
                NamesError::Full => TypesError::TooManyTags,
                NamesError::Repeated => unreachable!("the match names no tag {name} yet"),
            })
    }

    /// Gives `unit_type` the tag `tag` too, as the engine tags a type by its sections.
    pub fn give_tag(&mut self, unit_type: UnitType, tag: Tag) {
        let tags = &mut self.type_tags[unit_type.index()];
        *tags = tags.with(tag);
    }

    /// Tags each unit type of `bodies`, each by the body it has, if any, with the name of the
    /// layer it moves on, when `layers` names that layer: the first with no body.
    pub fn tag_layers(
        &mut self,
        layers: &Names<Layer>,
        bodies: impl IntoIterator<Item = (UnitType, Option<BodyForm>)>,
    ) {
        for (unit_type, body) in bodies {
            let layer = body.map_or(Layer::FIRST, BodyForm::layer);
            if let Some(name) = layers.name(layer) {
                let tag = self
                    .tags
                    .named(name)
                    .expect("the match declared every tag its packages name");
                self.give_tag(unit_type, tag);
            }
        }
    }

    /// The types' names, each in its scope.
    pub const fn types(&self) -> &Arc<Names<UnitType>> {
        &self.types
    }

    /// The tags' names, the engine's then the match's, as a filter reads them.
    pub const fn tags(&self) -> &TagNames {
        &self.tags
    }

    /// The book of the names and params of the types and tags so far, as the release puts in
    /// before a match's books load.
    pub fn into_book(self) -> UnitTypeBook {
        let book = UnitTypeBook::new(
            self.tags.clone(),
            Arc::clone(&self.types),
            Arc::clone(&self.params),
        );
        #[cfg(feature = "internals")]
        let book = book.loading(self);
        book
    }

    /// The books of the types: their names and params, their tags, each with the effects `data`
    /// gives it by name beside an engine tag's own, their origins and their state fields. A tag
    /// neither names has no effect.
    pub fn finish(self, data: &BTreeMap<DeclaredName, TagData>) -> UnitTypeBooks {
        let tags = self.tags.names().iter().map(|(tag, name)| {
            let engine = self
                .engine
                .get(tag.index())
                .map_or(TagProperties::NONE, |tag| tag.properties());
            let Some(data) = data.get(name) else {
                return (engine, TagSet::NONE);
            };
            let immune = data.immune.iter().map(|name| {
                self.tags
                    .named(name.as_str())
                    .expect("the match declared every tag the mode names")
            });
            (engine.union(data.properties()), immune.collect())
        });
        let effects: Vec<_> = tags.collect();
        let types = self.type_tags.into_iter().enumerate().map(|(at, tags)| {
            (
                UnitType::new(u16::try_from(at).expect("types fit u16")),
                tags,
            )
        });
        UnitTypeBooks {
            types: UnitTypeBook::new(self.tags, self.types, self.params),
            tags: TagBook::new(self.engine.len(), effects, types),
            origins: self.origins,
            states: self.states.into_iter().collect(),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::unit_state_book::UnitStateBook;
    use crate::unit_types::UnitTypes;

    impl UnitTypes {
        /// The book of the state fields of the types so far, as a test that loads a type at a
        /// time puts it in again after each.
        pub(crate) fn state_book(&self) -> UnitStateBook {
            self.states.iter().cloned().collect()
        }
    }
}
