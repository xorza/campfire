use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;

/// On the root of a drawing: the unit it draws. The unit may be `Unpredicted`, which the renderer
/// does not see, so the drawing is an entity of its own; the unit's despawn takes it, and every
/// child of it, along.
#[derive(Component, Debug)]
#[relationship(relationship_target = Drawing)]
pub(crate) struct DrawingOf(pub(crate) Entity);

/// On a drawn unit: the root of its drawing, which stands on the ground at the unit's place, and
/// holds its figure, its gauges and the ring of a target.
#[derive(Component, Debug)]
#[relationship_target(relationship = DrawingOf, linked_spawn)]
pub(crate) struct Drawing(Entity);

impl Drawing {
    pub(crate) const fn root(&self) -> Entity {
        self.0
    }
}
