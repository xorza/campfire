use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_script::{MatchNames, TagNames};
use campfire_values::{NameTable, Names, Scalar, UnitType};

/// The names of the unit types a match loaded and of their tags, and the types' params, which
/// `UnitTypes::finish` makes. Package data, not state: a restore loads it from the packages, as a
/// new match does. It gives the script view the types' and the tags' names and the types' params.
#[derive(Resource, Debug)]
pub struct UnitTypeBook {
    /// The name of each tag, by tag: the engine's, then the match's.
    tags: TagNames,
    /// The name of each type in its scope, by type; shared with the script view.
    types: Arc<Names<UnitType>>,
    /// Each type's params, one run per type, in the order of the types; shared with the script
    /// view.
    params: Arc<NameTable<Scalar>>,
    /// The types a test match loads one at a time, until `Units::seal_tags` finishes them.
    #[cfg(feature = "internals")]
    #[expect(
        clippy::absolute_paths,
        reason = "a gated field names its type in full"
    )]
    loading: Option<Box<crate::unit_types::UnitTypes>>,
}

impl UnitTypeBook {
    pub(crate) const fn new(
        tags: TagNames,
        types: Arc<Names<UnitType>>,
        params: Arc<NameTable<Scalar>>,
    ) -> Self {
        Self {
            tags,
            types,
            params,
            #[cfg(feature = "internals")]
            loading: None,
        }
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
}

impl Book for UnitTypeBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(&self.types));
        names.set_tags(self.tags.clone());
        names.set_params(Arc::clone(&self.params));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::unit_type_book::UnitTypeBook;
    use crate::unit_types::UnitTypes;

    impl UnitTypeBook {
        /// How many unit types the match loaded, for a test to name the next one.
        pub fn count(&self) -> usize {
            self.types.len()
        }

        /// The book, which keeps `types`, whose book it is, so a test loads more of them one at
        /// a time.
        pub(crate) fn loading(self, types: UnitTypes) -> Self {
            Self {
                loading: Some(Box::new(types)),
                ..self
            }
        }

        /// The types a test match loads, which their book holds until they are finished, and
        /// which the book of a match's packages never holds.
        pub(crate) fn take_loading(&mut self) -> UnitTypes {
            let types = self.loading.take();
            *types.expect("a test loads types only until they seal, into a match of no package")
        }

        /// Whether the book holds the types a test match loads.
        pub(crate) const fn is_loading(&self) -> bool {
            self.loading.is_some()
        }
    }
}
