use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, ByType, Fact, SourceKind};
use campfire_script::{StateSchema, TagNames};
use campfire_values::{
    EngineTag, NameId, NameTable, Names, NamesError, PackageIndex, Scalar, Tag, TagProperties,
    TagSet, TypeScope, UnitType,
};

use crate::avatar::Avatar;
use crate::body_form::BodyForm;
use crate::declared_tags::DeclaredTags;
use crate::error::TypesError;
use crate::layer::Layer;
use crate::layer_names::LayerNames;
use crate::tag_book::TagBook;
use crate::tag_data::{ModeTagsData, TagData};
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

impl Fact for UnitTypes {}

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

    /// The match's unit types and tags: the release's engine tags, then every tag the packages
    /// name, then each package's types, in the order `cx` numbers them, an avatar's tagged
    /// `avatar`; an error for a type its scope names already, or past the most types or tags a
    /// match holds.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let source = cx.source();
        let fail = |error| BookError::rule(PackageIndex::MODE, BookPlace::Types, error);
        let mut types = Self::new(source.engine_tags.iter().copied());
        for name in cx.fact::<DeclaredTags>()?.of_packages(source.engine_tags) {
            types.declare(name.as_str()).map_err(fail)?;
        }
        for (index, package) in cx.packages() {
            let scope = TypeScope::of_package(index);
            for (name, data) in package.units {
                let origin = TypeOrigin {
                    package: index,
                    name: name.as_str().into(),
                };
                types
                    .load(scope, origin, &UnitTypeCore::from(data))
                    .map_err(fail)?;
            }
            if let SourceKind::Avatar(data) = package.kind {
                let origin = TypeOrigin {
                    package: index,
                    name: package.name.into(),
                };
                types
                    .load(TypeScope::Mode, origin, &UnitTypeCore::from(data))
                    .map_err(fail)?;
            }
        }
        for unit in cx.types() {
            debug_assert_eq!(
                types.types.named((unit.scope, unit.name)),
                Some(unit.unit_type),
                "the build numbers the types as their load does"
            );
            if unit.is_avatar() {
                types.give_tag(unit.unit_type, Avatar::TAG.tag());
            }
        }
        Ok(types)
    }

    /// Tags each unit type that stands with the name of the layer its body moves on, as the
    /// mode's layers name them: the first with no body.
    pub fn tag_bodies(cx: &BookCx<'_>, types: &mut Self) -> Result<(), BookError> {
        let Some(layers) = cx.fact_if::<LayerNames>()? else {
            return Ok(());
        };
        let bodies = cx.book::<ByType<BodyForm>>()?;
        let standing = cx.standing().map(|unit| unit.unit_type);
        let bodies = standing.map(|unit_type| (unit_type, bodies.get(unit_type).copied()));
        types.tag_layers(&layers.0, bodies);
        Ok(())
    }

    /// The book of the match's unit types and tags, as their draft holds them.
    pub fn build_type_book(cx: &BookCx<'_>) -> Result<UnitTypeBook, BookError> {
        Ok(cx.fact::<Self>()?.type_book())
    }

    /// The book of the match's unit types' origins.
    pub fn build_origin_book(cx: &BookCx<'_>) -> Result<TypeOriginBook, BookError> {
        Ok(cx.fact::<Self>()?.origin_book())
    }

    /// The book of the match's unit types' state fields.
    pub fn build_state_book(cx: &BookCx<'_>) -> Result<UnitStateBook, BookError> {
        Ok(cx.fact::<Self>()?.state_book())
    }

    /// The book of the match's tags, each with the effects the mode's `[tags]` gives it.
    pub fn build_tag_book(cx: &BookCx<'_>) -> Result<TagBook, BookError> {
        let none = BTreeMap::new();
        let data = cx.mode_section::<ModeTagsData>();
        let data = data.map_or(&none, |data| &data.0);
        Ok(cx.fact::<Self>()?.tag_book(data))
    }

    /// The book of the types' names and params.
    pub fn type_book(&self) -> UnitTypeBook {
        UnitTypeBook::new(
            self.tags.clone(),
            Arc::clone(&self.types),
            Arc::clone(&self.params),
        )
    }

    /// The book of the types' tags, each with the effects `data` gives it by name beside an engine
    /// tag's own. A tag neither names has no effect.
    pub fn tag_book(&self, data: &BTreeMap<DeclaredName, TagData>) -> TagBook {
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
        let types = self.type_tags.iter().enumerate().map(|(at, &tags)| {
            (
                UnitType::new(u16::try_from(at).expect("types fit u16")),
                tags,
            )
        });
        TagBook::new(self.engine.len(), effects, types)
    }

    pub fn origin_book(&self) -> TypeOriginBook {
        self.origins.clone()
    }

    /// The book of the types' state fields.
    pub fn state_book(&self) -> UnitStateBook {
        self.states.iter().cloned().collect()
    }

    /// The books of the types: their names and params, their tags, each with the effects `data`
    /// gives it by name beside an engine tag's own, their origins and their state fields. A tag
    /// neither names has no effect.
    pub fn finish(self, data: &BTreeMap<DeclaredName, TagData>) -> UnitTypeBooks {
        UnitTypeBooks {
            tags: self.tag_book(data),
            states: self.state_book(),
            origins: self.origins,
            types: UnitTypeBook::new(self.tags, self.types, self.params),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use campfire_declare::BuiltBooks;

    use crate::tag_book::TagBook;
    use crate::unit_type_book::UnitTypeBook;
    use crate::unit_types::UnitTypes;

    impl UnitTypes {
        /// The book of the names and params of the types and tags so far, which keeps them, so a
        /// test loads more of them one at a time.
        pub(crate) fn into_book(self) -> UnitTypeBook {
            let book = UnitTypeBook::new(
                self.tags.clone(),
                Arc::clone(&self.types),
                Arc::clone(&self.params),
            );
            book.loading(self)
        }

        /// Puts them in `built` in place of the books a match's sealed types give: their book,
        /// open for a test to load its types into, and so no tag yet, as their seal gives them.
        pub fn open_in(self, built: &mut BuiltBooks) {
            built
                .take::<UnitTypeBook>()
                .expect("the core declares the unit types");
            built.take::<TagBook>().expect("the core declares the tags");
            built.add(self.into_book());
            built.add(TagBook::new(0, [], []));
        }
    }
}
