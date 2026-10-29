use campfire_math::{Num, Vec3};
use campfire_units::{AccelStep, TurnStep};

use crate::progress::Progress;
use crate::route::RouteMutItem;
use crate::zero_hour::zero_hour_drive::ZeroHourDrive;
use crate::zero_hour::zero_hour_mover::{Stance, Steer, ZeroHourMover};

/// A Zero Hour walker as it drives a tick: its locomotor; its speed in meters a tick, its turn
/// and its acceleration, as its stats give them; where it stands and faces; and its drive.
#[derive(Debug)]
pub(crate) struct ZeroHourDriver<'a> {
    pub(crate) mover: &'a ZeroHourMover,
    pub(crate) speed: Num,
    pub(crate) turn: Option<TurnStep>,
    pub(crate) accel: Option<AccelStep>,
    pub(crate) stance: Stance,
    pub(crate) state: ZeroHourDrive,
}

impl ZeroHourDriver<'_> {
    /// Drives along `route` a tick, as Zero Hour's path follower feeds its locomotor
    /// (04-capabilities/navigation.md, The route feeds the mover): toward the route's next
    /// waypoint, with the length left along the route as its distance to its goal, then past each
    /// waypoint but the last it comes within its step of. Within its close enough distance of the
    /// route's end, it has arrived, and stands on the end, where Zero Hour's stops where it stands.
    /// With no waypoint, as its route waits for the planner, it stands.
    pub(crate) fn follow(
        &mut self,
        route: &mut RouteMutItem<'_, '_>,
        progress: &mut Progress,
        open: impl Fn(Vec3) -> bool,
    ) {
        let Self {
            mover,
            speed,
            turn,
            accel,
            stance,
            state,
        } = self;
        let ahead = route.read().ahead();
        let (Some(&next), Some(&end)) = (ahead.first(), ahead.last()) else {
            return;
        };
        let on_path = ahead
            .windows(2)
            .fold(stance.at.ground().distance(next.ground()), |length, leg| {
                length + leg[0].ground().distance(leg[1].ground())
            });
        if on_path < mover.close_enough() {
            while !route.read().ahead().is_empty() {
                route.advance();
            }
            progress.restart();
            stance.at = Vec3::new(end.x, stance.at.y, end.z);
            return;
        }
        let steer = Steer {
            speed: *speed,
            // A walker of no turn step turns at once, and one of no acceleration step gains its
            // speed in a tick.
            turn: turn.map_or(Num::int(180), TurnStep::get),
            accel: accel.map_or(*speed, AccelStep::get),
            goal: next,
            on_path,
        };
        mover.drive(steer, stance, state, open);
        while let [waypoint, _, ..] = route.read().ahead()
            && stance.at.within_ground(*waypoint, *speed)
        {
            route.advance();
            progress.restart();
        }
    }
}
