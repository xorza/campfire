use std::marker::PhantomData;

use bevy::ecs::resource::Resource;
use campfire_values::NameId;

/// The name players see of each id of the kind `I`, by its id: an action's, which the command
/// panel shows, or an item's, which the shop panel and the inventory bar show.
#[derive(Resource, Debug)]
pub(crate) struct NameTexts<I: NameId + Send + Sync + 'static> {
    names: Vec<String>,
    kind: PhantomData<fn() -> I>,
}

impl<I: NameId + Send + Sync + 'static> NameTexts<I> {
    /// The name of `id`.
    pub(crate) fn of(&self, id: I) -> &str {
        &self.names[id.index()]
    }
}

/// The names in the order of their ids.
impl<I: NameId + Send + Sync + 'static> FromIterator<String> for NameTexts<I> {
    fn from_iter<T: IntoIterator<Item = String>>(names: T) -> Self {
        Self {
            names: names.into_iter().collect(),
            kind: PhantomData,
        }
    }
}
