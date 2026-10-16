use bevy_ecs::entity::Entity;
use campfire_math::Vec3;
use campfire_sim::StableId;

use crate::gatherer::NodeAt;

/// What a worker does in its loop this tick.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Step {
    /// Nothing changes.
    Hold,
    /// It walks to the point.
    Walk(Vec3),
    /// It stands where it is, its step as it was.
    Stand,
    /// Its loop ends.
    Stop,
    /// Its gather ends with no load, as a tag blocks its `use` group, and its node frees.
    Interrupt(Entity),
    /// It goes on to this node.
    Retarget(NodeAt),
    /// It waits at its node.
    Wait,
    /// It holds the node of this entity, and gathers.
    Take(Entity),
    /// Its gather of the node of this entity ends: it takes the most a trip carries.
    Collect(Entity),
    /// It takes its load to this drop-off.
    Choose(StableId),
    /// Its load joins its player's resources: then on to the node, or none to stop.
    Deliver(Option<NodeAt>),
}

impl Step {
    /// Whether the loop takes another step in the tick after this one.
    pub(super) const fn goes_on(self) -> bool {
        matches!(
            self,
            Self::Retarget(_) | Self::Collect(_) | Self::Choose(_) | Self::Deliver(_)
        )
    }
}
