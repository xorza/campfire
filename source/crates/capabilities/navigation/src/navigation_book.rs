use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_geometry::Grid;
use campfire_script::MatchNames;
use campfire_values::{Names, PathId};
use std::sync::Arc;

use crate::paths::Paths;

/// The map's paths, which walkers go along, and the cells units plan routes over. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct NavigationBook {
    pub paths: Paths,
    /// The cells units plan routes over, when the map has them.
    pub pathing: Option<Grid>,
}

impl NavigationBook {
    /// The names of the map's paths, which the load resolves them by.
    pub(crate) fn path_names(cx: &BookCx<'_>) -> Result<Arc<Names<PathId>>, BookError> {
        Ok(Arc::new(cx.book::<Self>()?.paths.names().clone()))
    }
}

impl Book for NavigationBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(self.paths.shared_names());
    }
}
