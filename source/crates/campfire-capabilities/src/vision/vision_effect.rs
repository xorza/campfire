use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::Position;

use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::units::team::Team;
use crate::vision::reveals::{Reveal, Reveals};
use crate::vision::vision_grid::VisionGrid;

/// A reveal a call queued: to `team`'s vision group, of the cells within `radius` of `pos`, in
/// `ticks` Vision stages from the tick it applies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisionEffect {
    pub(crate) team: Team,
    pub(crate) pos: Position,
    pub(crate) radius: Num,
    pub(crate) ticks: Ticks,
}

impl Effect for VisionEffect {
    // A match with no grid sees nothing, so it keeps no reveal.
    fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
        if !world.contains_resource::<VisionGrid>() {
            return;
        }
        let last = now.after(Ticks::new(self.ticks.get() - 1));
        world.resource_mut::<Reveals>().push(Reveal {
            team: self.team,
            pos: self.pos,
            radius: self.radius,
            last,
        });
    }
}
