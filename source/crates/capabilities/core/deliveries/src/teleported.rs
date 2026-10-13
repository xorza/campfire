use bevy_ecs::event::Event;
use campfire_sim::StableId;

/// A unit put elsewhere at once, which a delivery that homes on it loses: navigation triggers
/// it, and each capability whose deliveries home observes it, so neither names the other.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Teleported {
    pub id: StableId,
}
