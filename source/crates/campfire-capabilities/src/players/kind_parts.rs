use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::EntityWorldMut;

/// What a module puts on an entity of the kind as it spawns.
type Part = Box<dyn Fn(&mut EntityWorldMut<'_>) + Send + Sync>;

/// The parts the installed modules declare for each entity of the kind `K`, the match's or a
/// player's, each made at the spawn from what the world holds then. Derived from the
/// declarations: a restore takes the parts from its snapshot.
#[derive(Resource)]
pub(crate) struct KindParts<K: Send + Sync + 'static> {
    parts: Vec<Part>,
    kind: PhantomData<K>,
}

impl<K: Send + Sync + 'static> Default for KindParts<K> {
    fn default() -> KindParts<K> {
        KindParts {
            parts: Vec::new(),
            kind: PhantomData,
        }
    }
}

impl<K: Send + Sync + 'static> KindParts<K> {
    /// Adds `part`, which each entity of the kind gets as it spawns.
    pub(crate) fn add(&mut self, part: impl Fn(&mut EntityWorldMut<'_>) + Send + Sync + 'static) {
        self.parts.push(Box::new(part));
    }

    /// Puts every part on `entity`, in the order the modules declared them.
    pub(crate) fn put_on(&self, entity: &mut EntityWorldMut<'_>) {
        for part in &self.parts {
            part(entity);
        }
    }
}

impl<K: Send + Sync + 'static> fmt::Debug for KindParts<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("KindParts").field(&self.parts.len()).finish()
    }
}
