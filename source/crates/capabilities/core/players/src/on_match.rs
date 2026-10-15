use bevy_ecs::change_detection::Ref;
use bevy_ecs::component::Component;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId, Unpredicted};

/// The match's `C`, on its entity, as a system reads it: a client holds the entity as the server
/// sends it, unpredicted.
#[derive(SystemParam, Debug)]
pub struct OnMatch<'w, 's, C: Component> {
    index: Res<'w, EntityIndex>,
    held: Query<'w, 's, Ref<'static, C>, Allow<Unpredicted>>,
}

impl<C: Component> OnMatch<'_, '_, C> {
    /// The match's `C`, which its entity holds from the match's start, with whether it changed
    /// since the system last ran.
    pub fn get(&self) -> Ref<'_, C> {
        let entity = self
            .index
            .get(StableId::MATCH)
            .expect("a match that started has its entity");
        self.held.get(entity).expect("the match's entity holds it")
    }
}
