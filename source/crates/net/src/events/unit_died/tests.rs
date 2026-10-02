use campfire_capabilities::Fallen;
use campfire_log::internals::round_trip;
use campfire_sim::IdAllocator;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    let mut ids = IdAllocator::default();
    let [killer, unit] = [ids.allocate(), ids.allocate()];
    let avatar = UnitDied {
        tick: Tick::new(7),
        unit,
        team: Some(Team::new(1)),
        owner: Some(PlayerSlot::new(0)),
        killer: Some(killer),
    };
    round_trip(&avatar);
    // A unit with no team, owner or killer leaves those fields out.
    round_trip(&UnitDied {
        team: None,
        owner: None,
        killer: None,
        ..avatar
    });
}

#[test]
fn a_death_logs_its_unit_as_it_fell_and_its_killer() {
    let mut ids = IdAllocator::default();
    let [killer, unit] = [ids.allocate(), ids.allocate()];
    let fallen = Fallen {
        unit,
        team: Some(Team::new(1)),
        owner: Some(PlayerSlot::new(0)),
    };
    let death = DeathView {
        fallen,
        killer: Some(killer),
        assisters: &[],
    };
    assert_eq!(
        UnitDied::of(Tick::new(7), &death),
        UnitDied {
            tick: Tick::new(7),
            unit,
            team: Some(Team::new(1)),
            owner: Some(PlayerSlot::new(0)),
            killer: Some(killer),
        }
    );
}
