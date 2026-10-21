use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;

/// On a mesh that draws a unit, under its figure or one of its models: the root of the unit's
/// drawing, which the pointer picks when its ray meets the mesh's triangles.
#[derive(Component, Debug)]
#[relationship(relationship_target = PickParts)]
pub(super) struct PickPartOf(pub(super) Entity);

/// On a unit's drawing root: the meshes that draw it, as the pointer casts its ray at them.
#[derive(Component, Debug)]
#[relationship_target(relationship = PickPartOf)]
pub(super) struct PickParts(Vec<Entity>);

impl PickParts {
    /// The meshes.
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = Entity> {
        self.0.iter().copied()
    }
}
