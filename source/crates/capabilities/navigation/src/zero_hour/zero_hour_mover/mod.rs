use std::num::NonZeroI64;

use campfire_math::{Num, Rounding, SinCos, Vec3};
use campfire_sim::MatchRate;
use campfire_units::Facing;

use crate::zero_hour::zero_hour_drive::ZeroHourDrive;
use crate::zero_hour::zero_hour_locomotor::{LocomotorAppearance, ZeroHourLocomotor};

/// A Zero Hour locomotor as a match drives it, each number at the match's rate: its appearance;
/// its braking in meters a tick each tick; its least and its turning speed in meters a tick; its
/// pivot in meters ahead of its center; whether it backs; the meters past which it turns in three
/// points; its lateral friction; the meters within which it has arrived; whether it pushes
/// itself out of a cell it may not stand in; the speed at which a
/// braking mover lands, at least, `MIN_VEL`, and below which a velocity's axis stops, the
/// physics' `THRESH`, each in meters a tick; and the ticks a wheel looks ahead in a turn,
/// `PROJECT_FRAMES`, and waits near its goal before it brakes, `DONUT_TIME_DELAY_SECONDS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZeroHourMover {
    appearance: LocomotorAppearance,
    braking: Num,
    min_speed: Num,
    min_turn_speed: Num,
    pivot: Num,
    backwards: bool,
    three_point: Num,
    lateral_friction: Num,
    close_enough: Num,
    corrects: bool,
    landing: Num,
    still: Num,
    projected: i64,
    donut: u32,
}

/// What a mover drives toward this tick: the speed, in meters a tick, the turn, in degrees a tick,
/// and the gain of speed each tick, its stats give it; its route's next waypoint; and the meters
/// left along its route to the route's end.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Steer {
    pub(crate) speed: Num,
    pub(crate) turn: Num,
    pub(crate) accel: Num,
    pub(crate) goal: Vec3,
    pub(crate) on_path: Num,
}

/// Where a mover stands and the way it faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stance {
    pub(crate) at: Vec3,
    pub(crate) facing: Facing,
}

/// A pathfinding cell's side, `PATHFIND_CELL_SIZE_F`, in meters.
const CELL: Num = Num::int(10);

/// The meters from its route's end past which a wheel's donut wait begins again,
/// `DONUT_DISTANCE`.
const DONUT_DISTANCE: Num = Num::int(40);

/// The speed a braking mover lands at, at least, a cell a second, `MIN_VEL`.
const LANDING_SPEED: Num = CELL;

/// The meters within which a turn about a pivot twitches, so the mover does not turn, 0.1, to the
/// nearest bit.
const TWITCH: Num = Num::from_bits(1_677_722);

/// The meters from its goal past which a braking mover lands, 0.001, to the nearest bit.
const TINY: Num = Num::from_bits(16_777);

/// The share of a turn's slow-down past which a tread near its goal slows, 0.05, to the nearest
/// bit.
const NEAR_TURN: Num = Num::from_bits(838_861);

/// The braking factor of a mover as it starts to brake, 1.1, to the nearest bit.
const START_FACTOR: Num = Num::from_bits(18_454_938);

/// The turn in degrees past which a tread or a leg stands to turn, `PI / 4`.
const STANDING_TURN: Num = Num::int(45);

/// The turn in degrees past which a wheel backs toward its goal, `PI / 2`.
const BACKING_TURN: Num = Num::int(90);

/// The turn in degrees past which a wheel slows to its turning speed, `PI / 20`.
const SMALL_TURN: Num = Num::int(9);

/// The turn in degrees past which a wheel looks ahead along its arc, `PI / 12`.
const ARC_TURN: Num = Num::int(15);

/// The speed away from a cell it may not stand in past which a mover leaves it already, 0.25 m a
/// tick, as `fixInvalidPosition` tests it.
const LEAVING: Num = Num::QUARTER;

/// The meters a leg aims ahead along the way it turns to, as `locoUpdate_moveTowardsAngle` aims.
const AIM: Num = Num::int(1000);

impl ZeroHourMover {
    /// `locomotor` at `rate`: each speed divided by the tick rate and the braking by its square,
    /// each rounded once; `THRESH`, 0.001 a frame at Zero Hour's 30 frames a second, 0.03 m/s.
    pub(crate) fn of(locomotor: &ZeroHourLocomotor, rate: MatchRate) -> Self {
        let hz = NonZeroI64::from(rate.hz()).get();
        let per_tick = |value: Num, by: i64| {
            value
                .checked_div_int(by, Rounding::NearestEven)
                .expect("a bounded value a tick fits")
        };
        let ticks = |ms| u32::try_from(rate.cut(ms).get()).expect("a wait of seconds fits u32");
        Self {
            appearance: locomotor.appearance,
            // A braking that rounds to 0 a tick brakes the least a `Num` holds, which every
            // slow-down distance divides by.
            braking: per_tick(locomotor.braking, hz * hz).max(Num::EPSILON),
            min_speed: per_tick(locomotor.min_speed, hz),
            min_turn_speed: per_tick(locomotor.min_turn_speed, hz),
            pivot: locomotor.pivot,
            backwards: locomotor.backwards,
            three_point: locomotor.three_point,
            lateral_friction: locomotor.lateral_friction,
            close_enough: locomotor.close_enough,
            corrects: locomotor.corrects,
            landing: per_tick(LANDING_SPEED, hz),
            still: Num::ONE
                .checked_mul_div_int(3, 100 * hz, Rounding::NearestEven)
                .expect("a hundredth of a meter fits"),
            projected: i64::from(ticks(500)),
            donut: ticks(2500),
        }
    }

    /// The meters from its route's end within which it has arrived.
    pub(crate) const fn close_enough(&self) -> Num {
        self.close_enough
    }

    /// The most a drive's count of ticks near its goal grows to: one past the wait, which braking
    /// then reads as passed.
    const fn donut_past(&self) -> u32 {
        self.donut + 1
    }

    /// Drives `stance` and `drive` a tick by `steer`, as `locoUpdate_moveTowardsPosition` and
    /// `PhysicsBehavior::update` move a ground unit a frame: a mover in a cell `open` refuses it
    /// pushes itself out, as `fixInvalidPosition`, and does no more; else its appearance's mover
    /// turns it and gives its push along its facing, and a mover that braked as its last tick
    /// ended lands toward its goal. Then its velocity gains the push and loses its lateral
    /// friction, and moves it unless it brakes. A wheel's arc ahead lies on cells `open` lets it
    /// stand in.
    pub(crate) fn drive(
        &self,
        steer: Steer,
        stance: &mut Stance,
        drive: &mut ZeroHourDrive,
        open: impl Fn(Vec3) -> bool,
    ) {
        drive.donut = drive.donut.saturating_add(1).min(self.donut_past());
        if self.corrects
            && !open(stance.at)
            && let Some(correction) = Self::correction(stance.at, drive.velocity, &open)
        {
            self.integrate(stance, drive, correction);
            return;
        }
        let stop_at_speed = (steer.speed / self.braking) * steer.speed / 2;
        if steer.on_path > CELL && steer.on_path > stop_at_speed {
            drive.braking = false;
            drive.factor = ZeroHourDrive::FACTOR;
        }
        let before = stance.at;
        let braked = drive.braked;
        let push = match self.appearance {
            LocomotorAppearance::Treads => self.treads(steer, stance, drive),
            LocomotorAppearance::TwoLegs => self.legs(steer, stance, drive),
            LocomotorAppearance::FourWheels | LocomotorAppearance::Motorcycle => {
                self.wheels(steer, stance, drive, &open)
            }
        };
        drive.braked = drive.braking;
        if braked {
            let goal = steer.goal.ground();
            let distance = before.ground().distance(goal);
            if distance > TINY {
                let speed = abs(Self::forward(drive.velocity, stance.facing)).max(self.landing);
                let landed = before.ground().step_toward(goal, speed.min(distance));
                stance.at += landed - before.ground();
            }
        }
        let [cos, minus_sin] = Self::way(stance.facing);
        self.integrate(stance, drive, [push * cos, push * minus_sin]);
    }

    /// Adds `gain`, `[x, z]` in meters a tick each tick, to the velocity of `drive` and takes its
    /// lateral friction, as `PhysicsBehavior::update` integrates a frame: an axis below the still
    /// speed stops, and the velocity moves `stance` unless its drive braked.
    fn integrate(&self, stance: &mut Stance, drive: &mut ZeroHourDrive, gain: [Num; 2]) {
        let [cos, minus_sin] = Self::way(stance.facing);
        let [x, z] = drive.velocity;
        // The way to its left, `(−d_y, d_x)` on Zero Hour's plane, whose y is −z.
        let left = [minus_sin, -cos];
        let lateral = x * left[0] + z * left[1];
        let friction = self.lateral_friction * lateral;
        let gained = |velocity: Num, gain: Num, side: Num| {
            let velocity = velocity + gain - friction * side;
            if abs(velocity) < self.still {
                Num::ZERO
            } else {
                velocity
            }
        };
        drive.velocity = [gained(x, gain[0], left[0]), gained(z, gain[1], left[1])];
        if !drive.braked {
            let [x, z] = drive.velocity;
            stance.at += Vec3::new(x, Num::ZERO, z);
        }
    }

    /// The push out of a cell it may not stand in of a mover at `at` of `velocity`, as
    /// `fixInvalidPosition`: a fifth of a meter a tick each tick away from each of the eight cells
    /// round it that `open` refuses, each a cell off, on each axis; and as much again as cancels its
    /// velocity toward them, the root of its speed into them, the game's own; none for a mover of no
    /// such cell round it, or one that leaves already, faster than a quarter meter a tick.
    fn correction(at: Vec3, [x, z]: [Num; 2], open: impl Fn(Vec3) -> bool) -> Option<[Num; 2]> {
        // The cells off on Zero Hour's plane, its y north, −z.
        let mut off = [0_i64; 2];
        for north in -1..=1_i64 {
            for east in -1..=1_i64 {
                let cell = at + Vec3::new(CELL * east, Num::ZERO, -(CELL * north));
                if !open(cell) {
                    off[0] -= east;
                    off[1] -= north;
                }
            }
        }
        if off == [0, 0] {
            return None;
        }
        let push = [Num::int(off[0]) / 5, -(Num::int(off[1]) / 5)];
        let way = Vec3::new(push[0], Num::ZERO, push[1])
            .normalized()
            .expect("a push of cells off is no zero");
        let toward = x * way.x + z * way.z;
        if toward > LEAVING {
            return None;
        }
        if toward >= Num::ZERO {
            return Some(push);
        }
        let cancel = (-toward).sqrt();
        Some([push[0] + way.x * cancel, push[1] + way.z * cancel])
    }

    /// The tread's turn and push, as `moveTowardsPositionTreads`: it turns first, slows by its
    /// turn's share of 45°, slows to 60 % within two cells of its goal while it turns, and brakes
    /// within its slow-down distance, harder the nearer it comes.
    fn treads(&self, steer: Steer, stance: &mut Stance, drive: &mut ZeroHourDrive) -> Num {
        // A turn that twitches leaves the turn its mover reads unset; it drives on as one of none.
        let turn = self
            .rotate(stance, steer.goal, steer.turn, drive.braking)
            .unwrap_or(Num::ZERO);
        let share = Self::share(turn);
        let mut goal = (Num::ONE - share) * steer.speed;
        let actual = Self::forward(drive.velocity, stance.facing);
        let slow = Self::slowing(actual) * (actual / self.braking);
        let near = stance
            .at
            .ground()
            .distance_order(steer.goal.ground(), CELL * 2);
        if near.is_lt() && share > NEAR_TURN {
            goal = Self::part(actual, 3, 5);
        }
        self.brake(steer.on_path, slow, slow, drive);
        if drive.braking {
            drive.factor = Self::factor(slow, steer.on_path);
            goal = self.braked(actual, slow, steer.on_path);
        }
        Self::push(goal - actual, steer.accel, -drive.factor * self.braking)
    }

    /// The leg's turn and push, as `moveTowardsPositionLegs`: it turns toward the way to its goal,
    /// slows by its turn's share of 45°, and slows to its least speed within its slow-down
    /// distance.
    fn legs(&self, steer: Steer, stance: &mut Stance, drive: &ZeroHourDrive) -> Num {
        let actual = Self::forward(drive.velocity, stance.facing);
        let way = Self::toward(stance.at, steer.goal);
        let turn = stance.facing.turn_to(way);
        let [cos, minus_sin] = Self::way(way);
        let aim = stance.at + Vec3::new(cos * AIM, Num::ZERO, minus_sin * AIM);
        self.rotate(stance, aim, steer.turn, drive.braking);
        let goal = if steer.on_path < self.slow_down(actual) {
            self.min_speed
        } else {
            (Num::ONE - Self::share(turn)) * steer.speed
        };
        Self::push(goal - actual, steer.accel, -self.braking)
    }

    /// The wheel's turn and push, as `moveTowardsPositionWheels`: from a stop toward a goal behind
    /// it, it backs, and turns in three points when its goal lies farther than its distance for
    /// them; past 9° it slows to its turning speed; past 15° it turns where it stands while its
    /// arc half a second ahead leaves the cells it may stand in; it brakes within its slow-down
    /// distance or once it drove near its goal for its donut wait; and it turns as far as its
    /// speed lets it, up to its turning speed.
    fn wheels(
        &self,
        steer: Steer,
        stance: &mut Stance,
        drive: &mut ZeroHourDrive,
        open: impl Fn(Vec3) -> bool,
    ) -> Num {
        let turn_speed = self.min_turn_speed.max(steer.speed / 4);
        let mut way = Self::toward(stance.at, steer.goal);
        let mut turn = stance.facing.turn_to(way);
        let mut actual = Self::forward(drive.velocity, stance.facing);
        if actual == Num::ZERO {
            drive.backwards = false;
            if self.backwards && abs(turn) > BACKING_TURN {
                drive.backwards = true;
                drive.three_point = steer.on_path > self.three_point;
            }
        }
        let mut backs = false;
        let mut three_point = false;
        if drive.backwards {
            if abs(turn) < BACKING_TURN {
                drive.backwards = false;
            } else {
                backs = true;
                drive.three_point = steer.on_path > self.three_point;
                three_point = drive.three_point;
                if !three_point {
                    way = Facing::of(way.degrees() + Num::int(180));
                    turn = stance.facing.turn_to(way);
                }
            }
        }
        let mut goal = steer.speed;
        if abs(turn) > SMALL_TURN {
            goal = goal.min(turn_speed);
        }
        if backs {
            actual = -actual;
        }
        let time = actual / self.braking + Num::ONE;
        let slow = Self::slowing(actual) * time + actual;
        if abs(turn) > ARC_TURN {
            let mean = (goal + actual) / 2;
            let distance = mean * self.projected;
            let share = Self::turn_share(mean, turn_speed);
            let swing = share * self.projected * steer.turn / 4;
            let swung = if turn < Num::ZERO { -swing } else { swing };
            let [cos, minus_sin] = Self::way(Facing::of(stance.facing.degrees() + swung));
            let ahead = |part: i64| {
                let reach = distance / part;
                stance.at + Vec3::new(cos * reach, Num::ZERO, minus_sin * reach)
            };
            if !open(ahead(2)) || !open(ahead(1)) {
                self.rotate(stance, steer.goal, steer.turn, drive.braking);
                return Num::ZERO;
            }
        }
        self.brake(steer.on_path, slow.max(CELL), slow, drive);
        if steer.on_path > DONUT_DISTANCE {
            drive.donut = 0;
        } else if drive.donut > self.donut {
            drive.braking = true;
        }
        if drive.braking {
            drive.factor = ZeroHourDrive::FACTOR;
            goal = self.braked(actual, slow, steer.on_path);
        }
        let swing = abs(Self::turn_share(actual, turn_speed)) * steer.turn;
        if backs && !three_point {
            let behind = stance.at - stance.at.ground_offset(steer.goal);
            self.rotate(stance, behind, swing, drive.braking);
        } else {
            self.rotate(stance, steer.goal, swing, drive.braking);
        }
        let braking = drive.factor * self.braking;
        if backs {
            Self::push(actual - goal, braking, -steer.accel)
        } else {
            Self::push(goal - actual, steer.accel, -braking)
        }
    }

    /// Starts to brake within `start` of the route's end, at the factor of a start, and stops
    /// past a cell and twice `slow`.
    fn brake(&self, on_path: Num, start: Num, slow: Num, drive: &mut ZeroHourDrive) {
        if on_path < start && !drive.braking {
            drive.braking = true;
            drive.factor = START_FACTOR;
        }
        if on_path > CELL && on_path > slow * 2 {
            drive.braking = false;
        }
    }

    /// The speed a braking mover aims at, at `actual` with its slow-down distance `slow`, `on_path`
    /// from its route's end: a tick's braking less past the end, half of it within a quarter of
    /// it, else its speed, never below 0.
    fn braked(&self, actual: Num, slow: Num, on_path: Num) -> Num {
        if slow > on_path {
            (actual - self.braking).max(Num::ZERO)
        } else if slow > Self::part(on_path, 3, 4) {
            (actual - self.braking / 2).max(Num::ZERO)
        } else {
            actual
        }
    }

    /// The meters a leg at `actual` takes to slow to its least speed at its braking,
    /// `calcSlowDownDist`, with its fudge of 1.05; 0 at or below its least speed.
    fn slow_down(&self, actual: Num) -> Num {
        let over = actual - self.min_speed;
        if over <= Num::ZERO {
            return Num::ZERO;
        }
        Self::part(over * over / self.braking, 21, 40)
    }

    /// Turns `stance` toward `target` by at most `most` degrees, as `rotateObjAroundLocoPivot`:
    /// about its pivot, which moves its center, but about its center while it brakes; the turn to
    /// the target it read, `None` for a target within the twitch of its pivot, where it does not
    /// turn.
    fn rotate(&self, stance: &mut Stance, target: Vec3, most: Num, braking: bool) -> Option<Num> {
        let pivot = if braking { Num::ZERO } else { self.pivot };
        let [cos, minus_sin] = Self::way(stance.facing);
        let from = stance.at + Vec3::new(cos * pivot, Num::ZERO, minus_sin * pivot);
        let offset = from.ground_offset(target);
        if pivot != Num::ZERO && abs(offset.x) < TWITCH && abs(offset.z) < TWITCH {
            return None;
        }
        let turn = stance.facing.turn_to(Self::toward(from, target));
        let turned = Facing::of(stance.facing.degrees() + turn.clamp(-most, most));
        let [turned_cos, turned_minus_sin] = Self::way(turned);
        stance.at += Vec3::new(
            (cos - turned_cos) * pivot,
            Num::ZERO,
            (minus_sin - turned_minus_sin) * pivot,
        );
        stance.facing = turned;
        Some(turn)
    }

    /// The forward speed of `velocity` along `facing`, as `getForwardSpeed2D` measures it, each
    /// axis apart: `√((vₓdₓ)² + (v_zd_z)²)`, negative when the two sum below 0 (driving.md, The
    /// release game's forward speed).
    fn forward([x, z]: [Num; 2], facing: Facing) -> Num {
        let [cos, minus_sin] = Self::way(facing);
        let (along_x, along_z) = (x * cos, z * minus_sin);
        let speed = (along_x * along_x + along_z * along_z).sqrt();
        if along_x + along_z >= Num::ZERO {
            speed
        } else {
            -speed
        }
    }

    /// The way `facing` points, `[x, z]` on the ground plane.
    fn way(facing: Facing) -> [Num; 2] {
        let SinCos { sin, cos } = SinCos::of_degrees(facing.degrees());
        [cos, -sin]
    }

    /// The facing from `from` toward `to` on the ground plane, 0° for a point on it, as
    /// `Atan2Origin` gives.
    fn toward(from: Vec3, to: Vec3) -> Facing {
        let offset = from.ground_offset(to);
        Facing::along([offset.x, offset.z]).unwrap_or(Facing::of(Num::ZERO))
    }

    /// The share of a stand-still turn that `turn` degrees makes, `|turn| / 45°`, at most 1.
    fn share(turn: Num) -> Num {
        (abs(turn) / STANDING_TURN).min(Num::ONE)
    }

    /// The share of its full turn a wheel at `speed` makes at `turn_speed`, at most 1; a full one
    /// at a turning speed of 0.
    fn turn_share(speed: Num, turn_speed: Num) -> Num {
        speed
            .checked_div(turn_speed, Rounding::NearestEven)
            .map_or(Num::ONE, |share| share.min(Num::ONE))
    }

    /// `speed / 1.5`, the slowing of a slow-down distance, rounded once.
    fn slowing(speed: Num) -> Num {
        Self::part(speed, 2, 3)
    }

    /// The braking factor at `slow` from `on_path`: `(slow / on_path)²`, at most its most; the most
    /// at the route's end.
    fn factor(slow: Num, on_path: Num) -> Num {
        let ratio = slow.checked_div(on_path, Rounding::NearestEven);
        // Past 3, the square is past the most.
        match ratio {
            Some(ratio) if abs(ratio) <= Num::int(3) => {
                (ratio * ratio).min(ZeroHourDrive::MAX_FACTOR)
            }
            _ => ZeroHourDrive::MAX_FACTOR,
        }
    }

    /// `value × by / over`, rounded once.
    fn part(value: Num, by: i64, over: i64) -> Num {
        value
            .checked_mul_div_int(by, over, Rounding::NearestEven)
            .expect("a part of a bounded value fits")
    }

    /// The push toward `delta`, the speed a mover lacks: `up` to gain speed, `down` to lose it,
    /// each no more than `delta`, as a push that would pass it pushes it alone.
    fn push(delta: Num, up: Num, down: Num) -> Num {
        if delta == Num::ZERO {
            return Num::ZERO;
        }
        let push = if delta > Num::ZERO { up } else { down };
        if abs(push) > abs(delta) { delta } else { push }
    }
}

/// `value`'s distance from 0.
fn abs(value: Num) -> Num {
    value.max(-value)
}

#[cfg(test)]
mod tests;
