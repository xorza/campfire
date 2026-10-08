use std::fmt;

use bevy_ecs::resource::Resource;

use crate::state_registry::CopyQuery;

/// The query each registered type copies its values through, by its place in the registry, made
/// once as a world starts recording its changes, so a copy builds no query and matches only the
/// archetypes new since the last.
#[derive(Resource, Default)]
pub(super) struct CopyQueries(Vec<Option<Box<CopyQuery>>>);

impl CopyQueries {
    pub(super) fn push(&mut self, query: Option<Box<CopyQuery>>) {
        self.0.push(query);
    }

    /// The query of the type at `at`; none for one that copies through none.
    pub(super) fn get_mut(&mut self, at: usize) -> Option<&mut CopyQuery> {
        self.0[at].as_deref_mut()
    }
}

/// The queries are type-erased; their count is what tells one world's from another's.
impl fmt::Debug for CopyQueries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CopyQueries")
            .field("types", &self.0.len())
            .finish()
    }
}
