use crate::declare::entity_kind::EntityKind;

/// What reads the kinds of entity the core declares, as the network layer reads them to send
/// each kind to its audience.
pub trait KindVisitor {
    fn kind<K: EntityKind>(&mut self);
}
