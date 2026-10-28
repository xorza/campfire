use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;

/// A container's riders are called out as a capability takes it from the match: production, which
/// sells it, triggers it, and interaction, which holds the riders, observes it, so neither writes
/// the other's state (04-capabilities/interaction.md, A sold container lets its riders out).
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RidersCalledOut {
    pub entity: Entity,
    pub fate: RiderFate,
}

/// What becomes of the riders called out: each leaves at its container's exit time, as Zero
/// Hour's `OpenContain::onSelling` orders them out, or those still inside go with it, as its
/// `onDelete` destroys them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiderFate {
    Leave,
    Go,
}
