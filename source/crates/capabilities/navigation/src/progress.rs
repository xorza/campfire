use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::blocking::Touch;

/// How a walker has been getting on: the waypoint it walks toward and the nearest it came to it,
/// for how many ticks in a row it came no half a step nearer it than ever, how many routes to its
/// goal it asked as it was stuck since it last passed a waypoint, and how a Zero Hour mover's
/// brake holds it against the units it touches. Apart from its route, as it changes every tick a
/// unit walks.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    toward: Option<Nearest>,
    stuck: u32,
    stuck_asks: u8,
    blocked: Blocked,
}

/// How a Zero Hour mover's brake held it against a unit it touches, as `m_blockedFrames` and
/// `m_isBlockedAndStuck` count it: free; held the ticks in a row it was, one while it must turn
/// to its waypoint or drives slower than its brake; or stuck, held by a unit that stands as it
/// faced its waypoint, until it asks its route again (entries-and-blocking.md, A Zero Hour mover
/// is stuck by its block).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Blocked {
    #[default]
    Free,
    Held(u32),
    Stuck,
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

    /// How long a Zero Hour mover's brake holds it against a unit that moves before it asks its
    /// route again, `2 × LOGICFRAMES_PER_SECOND` frames (`AIInternalMoveToState::update`).
    const BLOCKED_MS: u64 = 2000;

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
        Self::ticks_in(rate, Self::STUCK_MS)
    }

    /// The ticks in a row, at `rate`, a walker stays stuck before it asks its route again.
    pub(crate) fn ask_after(rate: MatchRate) -> u32 {
        Self::ticks_in(rate, Self::STUCK_ASK_MS)
    }

    /// The ticks, at `rate`, of `ms`, one of its fixed times.
    fn ticks_in(rate: MatchRate, ms: u64) -> u32 {
        let ticks = rate.ticks(ms).expect("a fixed time fits").get();
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

    /// Notes that the waypoint `from` it walks toward, its route's last, moved to `to`, as the goal
    /// of a chase or of a walk into a unit moves: the walk goes on toward it, and the nearest it
    /// came grows by the move, the farthest the goal may have gone from where it stood then, so a
    /// walker that steps toward a goal that moves away comes nearer by its step, and one that
    /// stands does not (entries-and-blocking.md, A walk into a unit follows it).
    pub(crate) fn goal_moved(&mut self, from: Vec3, to: Vec3) {
        if let Some(nearest) = &mut self.toward
            && nearest.waypoint == from
        {
            *nearest = Nearest {
                waypoint: to,
                distance: nearest
                    .distance
                    .saturating_add(from.ground_offset(to).length()),
            };
        }
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

    /// Forgets the walk, as the walker took a goal of its own or stopped, or passed a waypoint,
    /// the way on that a stuck walker lacks, so it takes its next waypoint fresh.
    pub const fn restart(&mut self) {
        *self = Self {
            toward: None,
            stuck: 0,
            stuck_asks: 0,
            blocked: Blocked::Free,
        };
    }

    /// Counts an ask of its route again as the walker was stuck, and starts its stuck ticks again,
    /// so the next ask waits as long.
    pub(crate) const fn restart_stuck(&mut self) {
        *self = Self {
            toward: None,
            stuck: 0,
            stuck_asks: self.stuck_asks.saturating_add(1),
            blocked: Blocked::Free,
        };
    }

    /// Notes how a Zero Hour mover's brake held it this tick, as `processCollision` and
    /// `doLocomotor` count its block: free of a unit it `touch`es; held one tick where it must
    /// turn, not `facing` its waypoint within `PI / 30`, or, `held` false, drove slower than its
    /// brake; stuck where a unit it touches stands as it faces its waypoint; else held one more
    /// tick. Stuck it stays until it asks its route again.
    pub(crate) const fn block(&mut self, touch: Option<Touch>, facing: bool, held: bool) {
        self.blocked = match (self.blocked, touch) {
            (Blocked::Stuck, _) => Blocked::Stuck,
            (_, None) => Blocked::Free,
            (_, Some(_)) if !facing => Blocked::Held(1),
            (_, Some(Touch { stands: true })) => Blocked::Stuck,
            (Blocked::Held(ticks), Some(_)) if held => Blocked::Held(ticks.saturating_add(1)),
            (_, Some(_)) => Blocked::Held(1),
        };
    }

    /// Whether its block, at `rate`, makes it ask its route again: stuck, or held past
    /// `BLOCKED_MS`.
    pub(crate) fn blocked_out(&self, rate: MatchRate) -> bool {
        match self.blocked {
            Blocked::Free => false,
            Blocked::Held(ticks) => ticks > Self::ticks_in(rate, Self::BLOCKED_MS),
            Blocked::Stuck => true,
        }
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
        progress.restart();
        assert_eq!(progress.track(here, waypoint, step), 0);
        // A goal that moves 2 m on along x keeps the walk: the nearest grows by 2 m, so a step of
        // 1 m toward it, which leaves the walker 1 m farther from it than before, is half a step
        // nearer than the bound, and progress; standing as it moves 2 m more counts. A move of a
        // waypoint the walker does not walk toward changes nothing.
        let moved = waypoint + Vec3::from((2, 0, 0));
        progress.goal_moved(waypoint, moved);
        assert_eq!(progress.track(here + Vec3::from((1, 0, 0)), moved, step), 0);
        let farther = moved + Vec3::from((2, 0, 0));
        progress.goal_moved(moved, farther);
        assert_eq!(
            progress.track(here + Vec3::from((1, 0, 0)), farther, step),
            1
        );
        progress.goal_moved(detour, waypoint);
        assert_eq!(
            progress.track(here + Vec3::from((1, 0, 0)), farther, step),
            2
        );
        // A nearest of the most a number holds, as a snapshot may give, stays there as its goal
        // moves on, where the sum would pass it.
        let mut restored = Progress {
            toward: Some(Nearest {
                waypoint: farther,
                distance: Num::MAX,
            }),
            stuck: 0,
            stuck_asks: 0,
            blocked: Blocked::Free,
        };
        restored.goal_moved(farther, moved);
        assert_eq!(
            restored.toward.map(|nearest| nearest.distance),
            Some(Num::MAX)
        );
    }

    #[test]
    fn a_zero_hour_movers_block_holds_it_stuck_by_one_that_stands_or_past_two_seconds() {
        // At 30 ticks a second, 2 s is 60 ticks: held 60 ticks by one that walks it asks not,
        // the 61st it does; a turn, or a tick slower than its brake, holds it one tick again, and
        // a tick free none. Held by one that stands as it faces its waypoint, it is stuck at once,
        // and stays so, free or not, until it asks its route again.
        let rate = MatchRate::new(std::num::NonZeroU32::new(30).unwrap());
        let walks = Some(Touch { stands: false });
        let stands = Some(Touch { stands: true });
        let mut progress = Progress::default();
        for _ in 0..60 {
            progress.block(walks, true, true);
        }
        assert!(!progress.blocked_out(rate));
        progress.block(walks, true, true);
        assert!(progress.blocked_out(rate));
        for (touch, facing, held) in [
            (walks, false, true),
            (walks, true, false),
            (None, true, true),
        ] {
            let mut turned = progress;
            turned.block(touch, facing, held);
            let expected = touch.map_or(Blocked::Free, |_| Blocked::Held(1));
            assert_eq!(turned.blocked, expected, "{touch:?} {facing} {held}");
        }
        let mut stuck = Progress::default();
        stuck.block(stands, false, true);
        assert!(!stuck.blocked_out(rate));
        stuck.block(stands, true, true);
        stuck.block(None, true, true);
        assert!(stuck.blocked_out(rate));
        stuck.restart_stuck();
        assert!(!stuck.blocked_out(rate));
    }
}
