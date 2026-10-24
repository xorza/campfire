use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// How a walker has been getting on: the waypoint it walks toward and the nearest it came to it,
/// for how many ticks in a row it came no half a step nearer it than ever, and how many routes to
/// its goal it asked as it was stuck since it last passed a waypoint. Apart from its route, as it
/// changes every tick a unit walks.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    toward: Option<Nearest>,
    stuck: u32,
    stuck_asks: u8,
}

/// A waypoint a walker walks toward, and the nearest it came to it on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Nearest {
    waypoint: Vec3,
    distance: Num,
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

    /// Notes that the walker stands at `position` as this tick steers, bound for `waypoint`, with
    /// a step of `step`; the ticks in a row it came no half a step nearer its waypoint than ever,
    /// which only coming so near starts again, or passing a waypoint, not a detour nor a new route
    /// to a waypoint of their own in place of one not passed, whose first tick counts on; a
    /// waypoint taken fresh, as the walk starts or past a waypoint, counts nothing
    /// (04-capabilities/navigation.md, A walker progresses only when it comes nearer its waypoint).
    pub(crate) fn track(&mut self, position: Vec3, waypoint: Vec3, step: Num) -> u32 {
        let distance = position.ground_offset(waypoint).length();
        let fresh = Nearest { waypoint, distance };
        match &mut self.toward {
            Some(nearest) if nearest.waypoint == waypoint => {
                if distance + step / 2 <= nearest.distance {
                    *nearest = fresh;
                    self.stuck = 0;
                } else {
                    self.stuck = self.stuck.saturating_add(1);
                }
            }
            Some(nearest) => {
                *nearest = fresh;
                self.stuck = self.stuck.saturating_add(1);
            }
            None => self.toward = Some(fresh),
        }
        self.stuck
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

    /// Forgets the walk, as the walker took a goal of its own or stopped.
    pub const fn restart(&mut self) {
        *self = Self {
            toward: None,
            stuck: 0,
            stuck_asks: 0,
        };
    }

    /// Forgets the routes it asked as it was stuck and its stuck ticks, as it passed a waypoint,
    /// the way on that a stuck walker lacks, and takes its next waypoint fresh.
    pub(crate) const fn passed_waypoint(&mut self) {
        self.toward = None;
        self.stuck = 0;
        self.stuck_asks = 0;
    }

    /// Counts an ask of its route again as the walker was stuck, and starts its stuck ticks again,
    /// so the next ask waits as long.
    pub(crate) const fn restart_stuck(&mut self) {
        *self = Self {
            toward: None,
            stuck: 0,
            stuck_asks: self.stuck_asks.saturating_add(1),
        };
    }
}

impl SimComponent for Progress {
    const NAME: &'static str = "navigation.progress";
    type Host = UnitEntity;

    // Its waypoint lies within the world's bound, as every place does, its distance is no
    // negative, and it names no book.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.toward
            .is_none_or(|nearest| nearest.waypoint.within_bound() && nearest.distance >= Num::ZERO)
    }
}

impl Replication for Progress {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_coming_half_a_step_nearer_than_ever_is_progress_and_a_new_waypoint_counts_on() {
        // A step of 1 m, so half a step is 0.5 m, toward (10, 0) from the origin, 10 m off.
        let step = Num::ONE;
        let waypoint = Vec3::from((10, 0, 0));
        let at = |x: Num| Vec3::new(x, Num::ZERO, Num::ZERO);
        let mut progress = Progress::default();
        // The first tick of a walk counts nothing, with nothing to come nearer than.
        assert_eq!(progress.track(Vec3::ZERO, waypoint, step), 0);
        // 0.5 m nearer is progress. A walker slowed to 0.4 m a tick comes 0.8 m nearer than its
        // nearest in two ticks, which is progress too: 0.4 m on counts, 0.8 m on starts again;
        // a tick it stands counts, and 0.4 m and then 0.8 m on from there likewise.
        let fifths = |n: i64| Num::HALF + Num::int(n) / 5;
        let ticks = [
            (fifths(0), 0),
            (fifths(2), 1),
            (fifths(4), 0),
            (fifths(4), 1),
            (fifths(6), 2),
            (fifths(8), 0),
        ];
        for (x, stuck) in ticks {
            assert_eq!(progress.track(at(x), waypoint, step), stuck, "{x:?}");
        }
        // Back and forth by a step, no nearer than its nearest, counts each tick.
        let nearest = fifths(8);
        for (x, stuck) in [
            (nearest - Num::ONE, 1),
            (nearest, 2),
            (nearest - Num::ONE, 3),
        ] {
            assert_eq!(progress.track(at(x), waypoint, step), stuck, "{x:?}");
        }
        // A new waypoint, as a detour's, counts on; coming half a step nearer it starts again.
        let detour = Vec3::from((5, 0, 5));
        let here = at(nearest);
        assert_eq!(progress.track(here, detour, step), 4);
        let nearer = here + (detour - here).normalized().unwrap();
        assert_eq!(progress.track(nearer, detour, step), 0);
        // Passing a waypoint is progress: the count starts again, and the next waypoint, taken
        // fresh, counts nothing its first tick.
        assert_eq!(progress.track(here, detour, step), 1);
        progress.passed_waypoint();
        assert_eq!(progress.track(here, waypoint, step), 0);
    }
}
