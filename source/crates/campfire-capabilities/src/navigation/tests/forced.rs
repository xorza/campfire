use campfire_common::Ticks;

use super::*;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;

/// Half a meter.
const HALF: Num = Num::from_bits(ONE / 2);

impl Walk {
    fn at(&self, id: StableId) -> Position {
        *self.sim.get::<Position>(id)
    }

    fn forced(&self, id: StableId) -> Option<ForcedMove> {
        self.sim.try_get::<ForcedMove>(id).copied()
    }

    /// Applies `effect` as a call's would, before the next tick.
    fn apply(&mut self, effect: NavigationEffect) {
        let now = self.sim.now();
        effect.apply(&mut self.sim.world, &mut Frame::default(), now);
    }
}

#[test]
fn a_dash_moves_its_unit_exactly_to_a_point_or_to_the_unit_it_follows() {
    let mut walk = Walk::new();
    // A dash of 2 m a tick to (5, 9, 0) goes on the ground plane, at its own height: 2 m, 4 m,
    // then the last meter in tick 2, where it ends. Its unit keeps its destination, (0, 0, 9),
    // and takes no step of its own, so it stays on z = 0; once the dash ends it asks its route
    // there again in tick 2, and walks it from tick 3.
    let dasher = walk.unit(at(0, 0, 0), Some(at(0, 0, 9)));
    let to = DashTo::Point(at(5, 9, 0));
    walk.sim.insert(
        dasher,
        ForcedMove::Dash {
            to,
            step: Num::int(2),
        },
    );
    // A walker of 0.5 m walks a meter a tick along x from (10, 0, 20); a chaser of 0.5 m dashes 3
    // m a tick at it from (0, 0, 20). The walker steps first each tick: 11, 12, 13, 14, 15; the
    // chaser 3, 6, 9, 12, and in tick 4, 3 m off, within its step and the bodies' 1 m, it comes
    // to 14, where their bodies touch, and ends.
    let walker = walk.body(at(10, 0, 20), Some(at(30, 0, 20)), Some(Num::ONE), HALF);
    let chaser = walk.body(at(0, 0, 20), None, Some(Num::ONE), HALF);
    let to = DashTo::Unit(walker);
    walk.sim.insert(
        chaser,
        ForcedMove::Dash {
            to,
            step: Num::int(3),
        },
    );
    let mut dash_path = Vec::new();
    let mut chase_path = Vec::new();
    for tick in 0..5 {
        walk.sim.step();
        // A forced move under way is state, restored with the rest.
        if tick == 0 {
            let mut restored = Walk::new();
            walk.sim.restore_into(&mut restored.sim);
            assert_eq!(restored.forced(chaser), walk.forced(chaser));
        }
        dash_path.push(walk.at(dasher));
        chase_path.push((walk.at(chaser), walk.at(walker)));
    }
    let x = |x: i64| at(x, 0, 0);
    assert_eq!(dash_path, [x(2), x(4), x(5), dash_path[3], dash_path[4]]);
    assert_ne!(dash_path[3], x(5));
    assert_eq!(walk.sim.get::<Destination>(dasher).get(), Some(at(0, 0, 9)));
    let row = |x: i64| at(x, 0, 20);
    let pairs = [(3, 11), (6, 12), (9, 13), (12, 14), (14, 15)];
    assert_eq!(
        chase_path,
        pairs.map(|(chaser, walker)| (row(chaser), row(walker)))
    );
    assert_eq!([dasher, chaser].map(|id| walk.forced(id)), [None, None]);
}

#[test]
fn a_knock_back_moves_its_unit_away_by_equal_shares_and_a_new_move_replaces_it() {
    let mut walk = Walk::new();
    // 3 m over 3 ticks away from (0, 0, −2): along z, a meter a tick, the last tick on its end.
    // One off its own place goes along x.
    let off_point = walk.unit(at(0, 0, 0), None);
    let off_itself = walk.unit(at(10, 0, 0), None);
    for (unit, from) in [(off_point, at(0, 0, -2)), (off_itself, at(10, 0, 0))] {
        walk.apply(NavigationEffect::KnockBack {
            unit,
            from,
            distance: Num::int(3),
            ticks: Ticks::new(3),
        });
    }
    walk.sim.step();
    assert_eq!(walk.at(off_point), at(0, 0, 1));
    assert_eq!(walk.at(off_itself), at(11, 0, 0));
    // A dash of 2 m a tick to (11, 0, 4) replaces the second's knock back: (11, 0, 2), then
    // (11, 0, 4), where it ends.
    walk.apply(NavigationEffect::Dash {
        unit: off_itself,
        to: DashTo::Point(at(11, 0, 4)),
        step: Num::int(2),
    });
    walk.sim.step();
    assert_eq!(walk.at(off_point), at(0, 0, 2));
    assert_eq!(walk.at(off_itself), at(11, 0, 2));
    walk.sim.step();
    assert_eq!(walk.at(off_point), at(0, 0, 3));
    assert_eq!(walk.at(off_itself), at(11, 0, 4));
    assert_eq!(
        [off_point, off_itself].map(|id| walk.forced(id)),
        [None, None]
    );

    // A third of a meter a tick: 1 m over 3 ticks, each a share of what is left, 1/3 m rounded
    // once to 5 592 405 bits, then (2²⁴ − 5 592 405) / 2 = 5 592 405.5 to 5 592 406, and the
    // rest, 5 592 405 bits, in the last tick, on the end.
    let thirds = walk.unit(at(0, 0, 10), None);
    walk.apply(NavigationEffect::KnockBack {
        unit: thirds,
        from: at(-1, 0, 10),
        distance: Num::ONE,
        ticks: Ticks::new(3),
    });
    let mut moved = Vec::new();
    for _ in 0..3 {
        walk.sim.step();
        moved.push(walk.at(thirds).get().x.to_bits());
    }
    assert_eq!(moved, [5_592_405, 5_592_405 + 5_592_406, ONE]);
}

#[test]
fn a_forced_move_stops_before_a_static_body_and_on_the_bounds() {
    let mut walk = Walk::new();
    walk.sim.world.insert_resource(
        Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap(),
    );
    // A tower of 1 m at (3, 0, 0). A dash of 2 m a tick from (−3, 0, 0) to (4, 0, 0), its body
    // 0.5 m: −1, then 1; the step to 3 would come within 1.5 m of the tower, so it stays on 1
    // and ends.
    walk.body(at(3, 0, 0), None, None, Num::ONE);
    let dasher = walk.body(at(-3, 0, 0), None, Some(Num::ONE), HALF);
    walk.sim.insert(
        dasher,
        ForcedMove::Dash {
            to: DashTo::Point(at(4, 0, 0)),
            step: Num::int(2),
        },
    );
    // A knock back of 9 m over 3 ticks from (−2, 0, 2) along x: 1, then 4, on the bound, then
    // 7, past it: it stops on 4, and ends a tick early. Its way passes 2 m from the tower.
    let knocked = walk.unit(at(-2, 0, 2), None);
    walk.apply(NavigationEffect::KnockBack {
        unit: knocked,
        from: at(-3, 0, 2),
        distance: Num::int(9),
        ticks: Ticks::new(3),
    });
    let mut path = Vec::new();
    for _ in 0..3 {
        walk.sim.step();
        let forced = [dasher, knocked].map(|id| walk.forced(id).is_some());
        path.push(([dasher, knocked].map(|id| walk.at(id)), forced));
    }
    assert_eq!(
        path,
        [
            ([at(-1, 0, 0), at(1, 0, 2)], [true, true]),
            ([at(1, 0, 0), at(4, 0, 2)], [true, true]),
            ([at(1, 0, 0), at(4, 0, 2)], [false, false]),
        ]
    );
}

#[test]
fn a_teleport_puts_its_unit_where_it_may_stand_at_once_and_ends_its_move() {
    let mut walk = Walk::new();
    walk.load_pathing(Num::ONE, [-4, -4], [4, 4], vec![ground(Num::ZERO)]);
    walk.sim.world.insert_resource(
        Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap(),
    );
    // A tower of 1 m at the origin blocks the four cells round it, whose centers are √0.5 m off.
    walk.body(at(0, 0, 0), None, None, Num::ONE);
    let unit = walk.unit(at(-3, 0, -3), Some(at(3, 0, 3)));
    walk.sim.insert(
        unit,
        ForcedMove::Dash {
            to: DashTo::Point(at(3, 0, -3)),
            step: HALF,
        },
    );
    walk.sim.step();
    // On the tower it may not stand: the nearest cells it may, √2.5 m off, are eight, and the
    // lowest numbered, in row 2 from z = −4 and column 3 from x = −4, has its center at
    // (−0.5, 0, −1.5), at the height asked. It ends the dash, and asks its route again.
    walk.apply(NavigationEffect::Teleport {
        unit,
        to: at(0, 2, 0),
    });
    let center = Position::new(Vec3::new(-HALF, Num::int(2), Num::from_bits(-3 * ONE / 2)));
    assert_eq!(walk.at(unit), center.unwrap());
    assert_eq!(walk.forced(unit), None);
    assert_eq!(walk.get_route(unit).asked(), Some(Tick::new(1)));
    // Past the bounds, on the nearest point of them, where it may stand.
    walk.apply(NavigationEffect::Teleport {
        unit,
        to: at(9, 0, 1),
    });
    assert_eq!(walk.at(unit), at(4, 0, 1));
}
