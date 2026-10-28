use bevy_ecs::world::World;
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::{EntityIndex, StableId};

use crate::riders::Riders;

/// A change of interaction a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionEffect {
    /// The acting unit, a rider whose entry resolved, enters `container` when the container
    /// takes it (transports.md, An entry is an action of the container's).
    Enter { container: StableId },
}

impl Effect for InteractionEffect {
    const KIND: EffectKind = EffectKind::Change;

    fn apply(self, world: &mut World, frame: &mut Frame) {
        match self {
            Self::Enter { container } => {
                let rider = frame
                    .acting()
                    .expect("an entry's resolve runs for its rider");
                let index = world.resource::<EntityIndex>();
                if let (Some(rider), Some(container)) = (index.get(rider), index.get(container)) {
                    Riders::enter(world, rider, container);
                }
            }
        }
    }
}
