use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_sim::SimResource;
use campfire_values::{NameId, Team};
use serde::{Deserialize, Serialize};

use crate::vision_book::VisionBook;

/// The reveals under way, in the order made: each shows its team's vision group the grid cells
/// whose centers lie within its radius of its place, in each Vision stage to its last tick.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reveals(Vec<Reveal>);

/// One reveal: its team, not its group, as the groups follow the relations while it lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reveal {
    pub team: Team,
    pub position: Vec3,
    pub(crate) radius: Num,
    pub(crate) last: Tick,
}

impl Reveals {
    pub(crate) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, reveal: Reveal) {
        self.0.push(reveal);
    }

    /// Drops the reveals whose last tick came before `now`, then calls `reveal` with each one
    /// under way in `now`: a reveal stays to the tick after its last, so a fog filled again in that
    /// tick, as after a restore, sees what the last Vision stage saw.
    pub(crate) fn run(&mut self, now: Tick, mut reveal: impl FnMut(&Reveal)) {
        self.0.retain(|under_way| under_way.last >= now);
        self.0.iter().for_each(&mut reveal);
    }

    /// Calls `reveal` with each reveal that was under way in `then`, a tick whose Vision stage
    /// ran, changing nothing.
    pub(crate) fn each_of(&self, then: Tick, mut reveal: impl FnMut(&Reveal)) {
        self.0
            .iter()
            .filter(|under_way| under_way.last >= then)
            .for_each(&mut reveal);
    }
}

impl SimResource for Reveals {
    const NAME: &'static str = "vision.reveals";

    // A reveal of a team the grid's match lacks has no group, and one of a negative radius no
    // cells; a match with no grid makes none; its place lies within the world's bound, as every
    // place does.
    fn check(&self, world: &World) -> bool {
        let grid = world
            .get_resource::<VisionBook>()
            .and_then(VisionBook::grid);
        self.0.iter().all(|reveal| {
            reveal.radius >= Num::ZERO
                && grid.is_some_and(|grid| reveal.team.index() < grid.teams)
                && reveal.position.within_bound()
        })
    }
}
