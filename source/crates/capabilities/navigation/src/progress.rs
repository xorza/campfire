use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
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
    /// How long a walker moves less than half a step a tick before it is stuck and marks the
    /// walkers it touches: long enough that a walker yielding for a moment keeps its way.
    const STUCK_MS: u64 = 150;

    /// The ticks in a row it has moved less than half a step a tick.
    pub const fn stuck_ticks(&self) -> u32 {
        self.stuck
    }

    /// The ticks in a row, at `rate`, a walker that moves less than half a step a tick takes to be
    /// stuck.
    pub fn stuck_after(rate: MatchRate) -> u32 {
        let ticks = rate.ticks(Self::STUCK_MS).expect("a fixed time fits").get();
        u32::try_from(ticks).expect("a fixed time's ticks fit u32")
    }

    /// Notes that the walker stands at `position` as this tick steers, with a step of `step`; the
    /// ticks in a row it has moved less than half a step.
    pub(crate) fn track(&mut self, position: Vec3, step: Num) -> u32 {
        let half = step / 2;
        self.stuck = match self.last_at {
            Some(last) if last.within_ground(position, half) => self.stuck.saturating_add(1),
            _ => 0,
        };
        self.last_at = Some(position);
        self.stuck
    }

    /// Starts counting again, as the walker took a new way.
    pub(crate) const fn reset(&mut self) {
        self.stuck = 0;
    }

    /// Forgets the walk, as the walker asked for a new route or stopped.
    pub const fn restart(&mut self) {
        *self = Self {
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
        self.last_at.is_none_or(Vec3::within_bound)
    }
}

impl Replication for Progress {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
