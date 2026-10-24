use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// How a walker has been getting on: where it stood as the tick before steered, for how many
/// ticks in a row it has moved less than half a step a tick, and how many routes to its goal it
/// asked as it was stuck since it last passed a waypoint. Apart from its route, as it changes
/// every tick a unit walks.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    last_at: Option<Vec3>,
    stuck: u32,
    stuck_asks: u8,
}

impl Progress {
    /// How long a walker moves less than half a step a tick before it is stuck and marks the
    /// walkers it touches: long enough that a walker yielding for a moment keeps its way.
    const STUCK_MS: u64 = 150;

    /// How long a walker stays stuck, no detour freeing it, before it asks its route again: long
    /// enough that the walkers in its way move on, as asks at `STUCK_MS` from a lane's crowd of
    /// creeps held a 3v3 hero below its level, and as 0 A.D.'s `UnitMotion` asks a long path only
    /// after its short paths fail three times.
    const STUCK_ASK_MS: u64 = 1000;

    /// The routes a walker asks as it is stuck, with no waypoint passed between, before it stops:
    /// a route each `STUCK_ASK_MS`, about 5 s stuck, as 0 A.D.'s `UnitMotion` stops after 35
    /// failed moves of 200 ms (04-capabilities/navigation.md, A stuck walker plans again, then
    /// stops).
    pub(crate) const MOST_STUCK_ASKS: u8 = 5;

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

    /// The ticks in a row, at `rate`, a walker stays stuck before it asks its route again.
    pub(crate) fn ask_after(rate: MatchRate) -> u32 {
        let ticks = rate
            .ticks(Self::STUCK_ASK_MS)
            .expect("a fixed time fits")
            .get();
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

    /// Whether the route the walker asks is one it asked as it was stuck, and since passed no
    /// waypoint.
    pub(crate) const fn asked_stuck(&self) -> bool {
        self.stuck_asks > 0
    }

    /// Whether the walker asked as many routes as it may as it was stuck.
    pub(crate) const fn asked_out(&self) -> bool {
        self.stuck_asks >= Self::MOST_STUCK_ASKS
    }

    /// Forgets the walk, as the walker asked for a new route or stopped.
    pub const fn restart(&mut self) {
        *self = Self {
            last_at: None,
            stuck: 0,
            stuck_asks: 0,
        };
    }

    /// Forgets the routes it asked as it was stuck, as it passed a waypoint, the way on that a
    /// stuck walker lacks.
    pub(crate) const fn passed_waypoint(&mut self) {
        self.stuck_asks = 0;
    }

    /// Forgets the walk, as the walker asked its route again as it was stuck, and counts the ask.
    pub(crate) const fn restart_stuck(&mut self) {
        *self = Self {
            last_at: None,
            stuck: 0,
            stuck_asks: self.stuck_asks.saturating_add(1),
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
