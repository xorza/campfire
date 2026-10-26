#![expect(
    unsafe_code,
    reason = "Bevy's `BundleWriter` spawns the restore's entities by component id, an unsafe call (02-engine-core.md, Unsafe only where an outside API asks)"
)]

use std::alloc::Layout;
use std::collections::VecDeque;
use std::fmt;

use bevy_ecs::bundle::{BundleScratch, BundleWriter};
use bevy_ecs::component::ComponentId;
use bevy_ecs::ptr::OwningPtr;
use bevy_ecs::world::World;

use crate::sim_state::SimComponent;
use crate::stable_id::StableId;
use crate::state_registry::error::SnapshotError;

/// A snapshot's entities as a restore decodes them before it writes any: their stable ids, in
/// order, and each component type's values, each by its entity's id, in order. A write then puts
/// each entity in once, its stable id and every value of it together, in one move between
/// archetypes (02-engine-core.md, A restore writes each entity once).
#[derive(Default)]
pub(crate) struct DecodedState {
    ids: Vec<StableId>,
    components: Vec<Box<dyn DecodedValues>>,
}

/// One component type's decoded values, as the write takes them, each by its entity's id.
trait DecodedValues {
    /// The stable id of the entity of its next value; none once every value is taken.
    fn next_id(&self) -> Option<StableId>;

    /// Pushes its next value onto `writer`.
    ///
    /// # Safety
    ///
    /// `writer` writes an entity of the world whose component ids the values were decoded with.
    unsafe fn push_next(&mut self, writer: &mut BundleWriter<'_>);
}

/// The decoded values of `C`, its component `id` of the restored world.
struct Values<C> {
    id: ComponentId,
    values: VecDeque<(StableId, C)>,
}

impl<C: SimComponent> DecodedValues for Values<C> {
    fn next_id(&self) -> Option<StableId> {
        self.values.front().map(|&(id, _)| id)
    }

    unsafe fn push_next(&mut self, writer: &mut BundleWriter<'_>) {
        let (_, value) = self.values.pop_front().expect("a value to push");
        OwningPtr::make(value, |value| {
            // SAFETY: `value` holds a `C`, of the component `self.id`, which the caller's world
            // registered.
            unsafe { writer.push_component_by_id(self.id, value, Layout::new::<C>()) };
        });
    }
}

impl DecodedState {
    /// Takes the snapshot's entities, `ids`, in order.
    pub(crate) fn entities(&mut self, ids: Vec<StableId>) {
        self.ids = ids;
    }

    /// Takes the decoded values of `C`, in order of their entities' ids, of the component `C`
    /// registers in `world`.
    pub(crate) fn component<C: SimComponent>(
        &mut self,
        world: &mut World,
        values: VecDeque<(StableId, C)>,
    ) {
        let id = world.register_component::<C>();
        self.components.push(Box::new(Values { id, values }));
    }

    /// Writes each entity into `world`, in order of stable id, with every value of it, in one
    /// move. An error for a value of an entity the snapshot does not list.
    pub(crate) fn write(mut self, world: &mut World) -> Result<(), SnapshotError> {
        let stable_id = world.register_component::<StableId>();
        let mut scratch = BundleScratch::default();
        for &id in &self.ids {
            // A value before this entity's id is of none the list holds; checked before any
            // value of this entity is pushed, so none stays in the scratch unwritten.
            let unlisted = self
                .components
                .iter()
                .any(|values| values.next_id().is_some_and(|next| next < id));
            if unlisted {
                return Err(SnapshotError::UnknownEntity);
            }
            let mut writer = scratch.writer();
            OwningPtr::make(id, |id| {
                // SAFETY: `id` holds a `StableId`, of the component `stable_id` of `world`.
                unsafe { writer.push_component_by_id(stable_id, id, Layout::new::<StableId>()) };
            });
            for values in &mut self.components {
                if values.next_id() == Some(id) {
                    // SAFETY: the values were decoded with `world`'s component ids.
                    unsafe { values.push_next(&mut writer) };
                }
            }
            let mut entity = world.spawn_empty();
            // SAFETY: each value pushed is of a component of `world`, whose entity it writes.
            unsafe { writer.write(&mut entity) };
        }
        if self
            .components
            .iter()
            .any(|values| values.next_id().is_some())
        {
            return Err(SnapshotError::UnknownEntity);
        }
        Ok(())
    }
}

impl fmt::Debug for DecodedState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedState")
            .field("ids", &self.ids.len())
            .field("components", &self.components.len())
            .finish()
    }
}
