use bevy_ecs::event::Event;
use campfire_sim::StableId;

/// A unit that died, which a delivery or a hit aimed at it loses, as a respawn a tick later puts
/// the same unit back before they move again: combat triggers it, and each capability whose
/// deliveries or hits aim at a unit observes it, so neither names the other.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Died {
    pub id: StableId,
}
