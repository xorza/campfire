use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Position, SimComponent};
use serde::{Deserialize, Serialize};

/// How a walker has been getting on: where it stood as the tick before steered, and for how many
/// ticks in a row it has moved less than half a step a tick. Apart from its route, as it changes
/// every tick a unit walks.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    last_at: Option<Position>,
    stuck: u32,
}

impl Progress {
    /// Notes that the walker stands at `at` as this tick steers, with a step of `step`; the ticks
    /// in a row it has moved less than half a step.
    pub(crate) fn track(&mut self, at: Position, step: Num) -> u32 {
        let half = step / 2;
        self.stuck = match self.last_at {
            Some(last) if last.within_ground(at, half) => self.stuck.saturating_add(1),
            _ => 0,
        };
        self.last_at = Some(at);
        self.stuck
    }

    /// Starts counting again, as the walker took a new way.
    pub(crate) const fn reset(&mut self) {
        self.stuck = 0;
    }

    /// Forgets the walk, as the walker asked for a new route or stopped.
    pub(crate) const fn restart(&mut self) {
        *self = Progress {
            last_at: None,
            stuck: 0,
        };
    }
}

impl SimComponent for Progress {
    const NAME: &'static str = "navigation.progress";

    // Its decode keeps its position within the bound, and it names no book.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Kinded for Progress {
    const KIND: DataKind = DataKind::Prediction;
}
