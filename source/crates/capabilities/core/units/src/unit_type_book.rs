use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_script::{StateSchema, TagNames, View};
use campfire_sim::Capability;
use campfire_values::{
    DeclaredName, EngineTag, NameTable, Names, NamesError, Scalar, Tag, TagProperties, TagSet,
    TypeScope, UnitType,
};

use crate::body_form::BodyForm;
use crate::error::TypesError;
use crate::layer::Layer;
use crate::tag_book::TagBook;
use crate::tag_data::TagData;
use crate::type_origin_book::{TypeOrigin, TypeOriginBook};
use crate::unit_state_book::UnitStateBook;
use crate::unit_type_core::UnitTypeCore;

/// The unit types a match loaded: their names, their tags, their params and their state fields.
/// Package data, not state: a restore loads it from the packages, as a new match does. It gives
/// the script view the types' and the tags' names and the types' params.
#[derive(Resource, Debug)]
pub struct UnitTypeBook {
    /// The name of each tag, by tag: the engine's, then the match's.
    tags: TagNames,
    /// The engine's tags, the first of `tags`.
    engine: Vec<EngineTag>,
    /// Each type's own tags, by type, until the tag book takes them: from then on the book alone
    /// holds them, and a type loaded or tagged later is a bug.
    staged_tags: Option<Vec<TagSet>>,
    /// The name of each type in its scope, by type; shared with the script view.
    types: Arc<Names<UnitType>>,
    /// Each type's package and its name there, by type, until the origin book takes them.
    origins: Option<TypeOriginBook>,
    /// Each type's params, one run per type, in the order of the types; shared with the script
    /// view.
    params: Arc<NameTable<Scalar>>,
    /// Each type's script state, a schema per type, in the order of the types, until the state
    /// book takes them.
    states: Option<Vec<StateSchema>>,
}

/// No tag and no type, as a match's declaration puts the book in before the release gives it
/// the engine's tags.
impl Default for UnitTypeBook {
    fn default() -> UnitTypeBook {
        UnitTypeBook::new(Vec::new())
    }
}

impl UnitTypeBook {
    /// The engine's tags `engine` at their places, as the release lists them, and no type.
    pub fn new(engine: impl IntoIterator<Item = EngineTag>) -> UnitTypeBook {
        let engine: Vec<EngineTag> = engine.into_iter().collect();
        UnitTypeBook {
            tags: TagNames::new(&engine),
            engine,
            staged_tags: Some(Vec::new()),
            types: Arc::default(),
            origins: Some(TypeOriginBook::default()),
            params: Arc::default(),
            states: Some(Vec::new()),
        }
    }

    /// Loads `data` as the type `name` of `scope`, declared by the package at `package`: its tags
    /// join the match's, and its params are kept for `unit.params`; an error for a type its scope
    /// names already, or past the most types or tags a match holds.
    pub fn load(
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
        let run = Arc::make_mut(&mut self.params).push(params);
        debug_assert_eq!(run, unit_type.index(), "one run of params per type");
        let fields = data.state.iter();
        let fields = fields.map(|(name, field)| (name.as_str(), field.sync, &field.decl));
        let states = self
            .states
            .as_mut()
            .expect("a type loads only before the state book takes the schemas");
        states.push(StateSchema::new(fields));
        debug_assert_eq!(states.len(), unit_type.index() + 1, "one schema per type");
        self.staged_mut().push(tags);
        self.origins
            .as_mut()
            .expect("a type loads only before the origin book takes the origins")
            .push(TypeOrigin {
                package,
                name: name.into(),
            });
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
    pub fn contains(&self, unit_type: UnitType) -> bool {
        self.types.has(unit_type)
    }

    /// The types' names, each in its scope.
    pub const fn types(&self) -> &Arc<Names<UnitType>> {
        &self.types
    }

    /// The tags' names, the engine's then the match's, as a filter reads them.
    pub const fn tags(&self) -> &TagNames {
        &self.tags
    }

    /// The book of each type's package and its name there, as the client finds its look, which
    /// takes them, once.
    pub const fn origin_book(&mut self) -> TypeOriginBook {
        self.origins
            .take()
            .expect("the origin book takes the origins once")
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
    pub fn tag_book(&mut self, data: &BTreeMap<DeclaredName, TagData>) -> TagBook {
        let tags = self.tags.names().iter().map(|(tag, name)| {
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
            (engine.union(data.properties()), immune.collect())
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

    /// The book of every type's state fields, which takes them, once.
    pub fn state_book(&mut self) -> UnitStateBook {
        self.states
            .take()
            .expect("the state book takes the schemas once")
            .into_iter()
            .collect()
    }
}

impl Book for UnitTypeBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View) {
        let mut names = view.names_mut();
        names.set_table(Arc::clone(&self.types));
        names.set_tags(self.tags.clone());
        names.set_params(Arc::clone(&self.params));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use super::*;

    impl UnitTypeBook {
        /// How many unit types the match loaded, for a test to name the next one.
        pub fn count(&self) -> usize {
            self.types.len()
        }

        /// The book of the state fields of the types loaded so far, as a test that loads a type
        /// at a time puts it in again after each.
        pub fn state_book_so_far(&self) -> UnitStateBook {
            let states = self.states.as_ref().expect("the state book took no schema");
            states.iter().cloned().collect()
        }

        /// Whether the tag book took the types' tags, after which no type loads.
        pub(crate) const fn sealed(&self) -> bool {
            self.staged_tags.is_none()
        }
    }
}
