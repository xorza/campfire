use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimComponent};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use serde::{Deserialize, Serialize};

/// How a walker has been getting on: where it stood as the tick before steered, and for how many
/// ticks in a row it has moved less than half a step a tick. Apart from its route, as it changes
/// every tick a unit walks.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    last_at: Option<Vec3>,
    stuck: u32,
}

impl Progress {
    /// Notes that the walker stands at `pos` as this tick steers, with a step of `step`; the ticks
    /// in a row it has moved less than half a step.
    pub(crate) fn track(&mut self, pos: Vec3, step: Num) -> u32 {
        let half = step / 2;
        self.stuck = match self.last_at {
            Some(last) if last.within_ground(pos, half) => self.stuck.saturating_add(1),
            _ => 0,
        };
        self.last_at = Some(pos);
        self.stuck
    }

    /// Starts counting again, as the walker took a new way.
    pub(crate) const fn reset(&mut self) {
        self.stuck = 0;
    }

    /// Forgets the walk, as the walker asked for a new route or stopped.
    pub const fn restart(&mut self) {
        *self = Progress {
            last_at: None,
            stuck: 0,
        };
    }
}

impl SimComponent for Progress {
    const NAME: &'static str = "navigation.progress";
    type Host = UnitEntity;

    // Its place lies within the world's bound, as every place does, and it names no book.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.last_at
            .is_none_or(|point| Position::from(point).within_bound())
    }
}

impl Replication for Progress {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
