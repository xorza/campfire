use campfire_log::internals::round_trip;
use campfire_sim::IdAllocator;
use campfire_values::TagSet;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    let mut ids = IdAllocator::default();
    let [killer, unit] = [ids.allocate(), ids.allocate()];
    let avatar = UnitDied {
        tick: Some(Tick::new(7)),
        unit,
        team: Some(Team::new(1)),
        owner: Some(Seat::new(0)),
        killer: Some(killer),
        avatar: true,
    };
    round_trip(&avatar);
    // An owned unit that is no avatar logs at debug, its owner named.
    round_trip(&UnitDied {
        avatar: false,
        ..avatar
    });
    // A unit with no tick, team, owner or killer leaves those fields out.
    round_trip(&UnitDied {
        tick: None,
        team: None,
        owner: None,
        killer: None,
        avatar: false,
        ..avatar
    });
}

#[test]
fn a_death_logs_its_unit_as_it_fell_and_its_killer() {
    let mut ids = IdAllocator::default();
    let [killer, id] = [ids.allocate(), ids.allocate()];
    let fallen = Fallen {
        id,
        team: Some(Team::new(1)),
        owner: Some(Seat::new(0)),
        avatar: true,
        tags: TagSet::NONE,
    };
    let death = DeathView {
        fallen,
        killer: Some(killer),
        assisters: &[],
    };
    assert_eq!(
        UnitDied::of(Tick::new(7), &death),
        UnitDied {
            tick: Some(Tick::new(7)),
            unit: id,
            team: Some(Team::new(1)),
            owner: Some(Seat::new(0)),
            killer: Some(killer),
            avatar: true,
        }
    );
    // A client learns the same death at its own tick, or at none, with no killer.
    for tick in [Some(Tick::new(9)), None] {
        assert_eq!(
            UnitDied::learned(tick, fallen),
            UnitDied {
                tick,
                killer: None,
                ..UnitDied::of(Tick::new(7), &death)
            }
        );
    }
}
