use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Command;
use bevy::ecs::world::World;

use crate::view::glide::Glide;

/// On the root of a drawing: the unit it draws. The unit may be `Unpredicted`, which the renderer
/// does not see, so the drawing is an entity of its own; the unit's despawn takes it, and every
/// child of it, along, unless it was torn off the unit first.
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

/// On a unit whose drawing was torn off it: the client draws it no more.
#[derive(Component, Debug)]
pub(crate) struct Undrawn;

/// Tears the drawing `root` off its unit `unit`, as an aircraft's falls as a wreck of its own
/// (wrecks.md, An aircraft's death falls as a wreck): the drawing stands where it stood, follows
/// its unit no more and outlives it, and the unit is drawn no more, as Unreal's `TearOff` hands a
/// dying actor to its client.
#[derive(Debug)]
pub(crate) struct TearOff {
    pub(crate) unit: Entity,
    pub(crate) root: Entity,
}

impl Command for TearOff {
    type Out = ();

    fn apply(self, world: &mut World) {
        if let Ok(mut root) = world.get_entity_mut(self.root) {
            root.remove::<(DrawingOf, Glide)>();
        }
        if let Ok(mut unit) = world.get_entity_mut(self.unit) {
            unit.insert(Undrawn);
        }
    }
}

/// On each child of a drawing's root that draws its unit, its figure or one of its models: the
/// pointer picks the unit by the meshes under it, and not by a gauge or a ring under the root.
#[derive(Component, Debug)]
pub(crate) struct Drawn;
