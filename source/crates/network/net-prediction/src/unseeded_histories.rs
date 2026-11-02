use core::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;

/// The predicted entities whose confirmed history of `C` a late `Predicted` asked to seed from the
/// live value while no confirmation of theirs resolved to a tick: each frame's survey seeds them
/// again, so the rollback check compares `C` once one resolves. Each leaves the list as its seed
/// lands or the entity goes, so the list holds at most the predicted entities.
#[derive(Resource, Debug)]
pub(crate) struct UnseededHistories<C> {
    pub(crate) entities: Vec<Entity>,
    marker: PhantomData<fn() -> C>,
}

impl<C> Default for UnseededHistories<C> {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            marker: PhantomData,
        }
    }
}
