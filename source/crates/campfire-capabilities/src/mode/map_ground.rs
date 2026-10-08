use bevy_ecs::world::World;

use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::metric::Metric;
use crate::navigation::Navigation;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::units::relations::Relations;

/// What a client's prediction takes of the map as a match does, and no script reads: its metric,
/// its bounds, how its teams regard each other, and the cells units plan routes over.
#[derive(Debug)]
pub(crate) struct MapGround {
    pub(super) metric: Metric,
    pub(super) bounds: Bounds,
    pub(super) relations: Relations,
    pub(super) pathing: Option<Grid>,
    /// The map's walls, which block cells on the pathing grid; none without it.
    pub(super) walls: Vec<Wall>,
}

impl MapGround {
    /// Puts the ground in `world`, its pathing grid for the kinds of `walkers`.
    pub(crate) fn install(self, world: &mut World, walkers: Vec<Walker>) {
        let MapGround {
            metric,
            bounds,
            relations,
            pathing,
            walls,
        } = self;
        world.insert_resource(metric);
        world.insert_resource(bounds);
        world.insert_resource(relations);
        if let Some(pathing) = pathing {
            Navigation::load_pathing(world, pathing, &walls, walkers);
        }
    }
}
