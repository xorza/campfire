use bevy_ecs::component::Component;

use crate::entity_kind::EntityKind;

/// What reads the kinds of entity the core declares, as the network layer reads them to send
/// each kind to its audience, and the parts of the states their entities enter and leave.
pub trait KindVisitor {
    fn kind<K: EntityKind>(&mut self);

    /// `S`, the part of a state that an event starts and ends on an entity of the kind `K`, as
    /// `Dead` marks a death: it comes and goes by command after the entity spawned, where every
    /// other part comes with the spawn and stays.
    fn state<K: EntityKind, S: Component>(&mut self);
}
