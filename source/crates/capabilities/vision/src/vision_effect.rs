use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_math::Num;
use campfire_script::{Effect, Frame};
use campfire_sim::Position;
use campfire_values::Team;

use crate::reveals::{Reveal, Reveals};
use crate::vision_book::VisionBook;

/// A reveal a call queued: to `team`'s vision group, of the cells within `radius` of `pos`, in
/// `ticks` Vision stages from the tick it applies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisionEffect {
    pub team: Team,
    pub pos: Position,
    pub(crate) radius: Num,
    pub(crate) ticks: Ticks,
}

impl Effect for VisionEffect {
    // A match with no grid sees nothing, so it keeps no reveal.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        if world.resource::<VisionBook>().grid().is_none() {
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
