use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{Position, SimResource};
use serde::{Deserialize, Serialize};

use crate::units::team::Team;
use crate::vision::vision_grid::VisionGrid;

/// The reveals under way, in the order made: each shows its team's vision group the grid cells
/// whose centers lie within its radius of its place, in each Vision stage to its last tick.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reveals(Vec<Reveal>);

/// One reveal: its team, not its group, as the groups follow the relations while it lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reveal {
    pub(crate) team: Team,
    pub(crate) pos: Position,
    pub(crate) radius: Num,
    pub(crate) last: Tick,
}

impl Reveals {
    pub(crate) fn push(&mut self, reveal: Reveal) {
        self.0.push(reveal);
    }

    /// Calls `reveal` with each reveal under way in `now`, then drops those whose last tick it
    /// is.
    pub(crate) fn run(&mut self, now: Tick, mut reveal: impl FnMut(&Reveal)) {
        self.0.retain(|under_way| {
            reveal(under_way);
            under_way.last > now
        });
    }
}

impl SimResource for Reveals {
    const NAME: &'static str = "vision.reveals";

    // A reveal of a team the grid's match lacks has no group, and one of a negative radius no
    // cells; a match with no grid makes none.
    fn check(&self, world: &World) -> bool {
        let grid = world.get_resource::<VisionGrid>();
        self.0.iter().all(|reveal| {
            reveal.radius >= Num::ZERO
                && grid.is_some_and(|grid| usize::from(reveal.team.index()) < grid.teams)
        })
    }
}
