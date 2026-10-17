use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::{Book, BookCx, BookError, BookPlace, NamesListError, Section};
use campfire_script::View;
use campfire_values::{Names, NamesError, PackageIndex, ResourceId};

use crate::resources_data::ResourcesData;

/// The names of the players' resources, by resource, which an action's costs and the script view
/// name them by.
#[derive(Resource, Debug, Clone, Default)]
pub struct ResourceBook(pub Arc<Names<ResourceId>>);

impl ResourceBook {
    /// The names of the mode's `resources`; an error for a name twice, or past the most
    /// resources a match holds.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut names = Names::default();
        let resources = cx.mode_section::<ResourcesData>().into_iter();
        for name in resources.flat_map(|resources| &resources.0) {
            names.try_add(name.as_str()).map_err(|error| {
                let error = match error {
                    NamesError::Full => NamesListError::TooMany,
                    NamesError::Repeated => NamesListError::Repeated(name.clone()),
                };
                let at = BookPlace::Section(ResourcesData::KEY);
                BookError::rule(PackageIndex::MODE, at, error)
            })?;
        }
        Ok(Self(Arc::new(names)))
    }
}

impl Book for ResourceBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(&self.0));
    }
}
