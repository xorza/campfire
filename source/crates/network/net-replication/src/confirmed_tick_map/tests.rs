use super::*;

#[test]
fn the_map_drops_its_oldest_entries() {
    let mut map = ConfirmedTickMap::default();

    for i in 0..=(MAX_CONFIRMED_TICKS as u32) {
        map.record(RepliconTick::new(i), NetTick::from(i));
    }

    assert_eq!(map.get(RepliconTick::new(0)), None);
    // The newest message's tick is the newest replicon tick's, whatever order they came in.
    assert_eq!(
        map.newest_tick(),
        Some(NetTick::from(MAX_CONFIRMED_TICKS as u32))
    );
    assert_eq!(
        map.get(RepliconTick::new(MAX_CONFIRMED_TICKS as u32)),
        Some(NetTick::from(MAX_CONFIRMED_TICKS as u32))
    );
}

/// Replicon tick `2i` maps to tick `10i`, and recording 257 of them evicts replicon tick 0. A
/// mapped confirmation resolves to its tick; one older than every retained confirmed tick resolves
/// to the oldest retained tick, 10 for replicon tick 2; an unmapped one within or past the
/// retained range does not resolve.
#[test]
fn confirmed_state_tick_falls_back_to_the_retained_floor() {
    let mut map = ConfirmedTickMap::default();
    for i in 0..=(MAX_CONFIRMED_TICKS as u32) {
        map.record(RepliconTick::new(i * 2), NetTick::from(i * 10));
    }
    let confirmed = |tick| map.confirmed_state_tick(&ConfirmHistory::new(RepliconTick::new(tick)));

    assert_eq!(confirmed(20), Some(NetTick(100)));
    assert_eq!(confirmed(0), Some(NetTick(10)));
    assert_eq!(confirmed(1), Some(NetTick(10)));
    assert_eq!(confirmed(21), None);
    assert_eq!(confirmed(2 * MAX_CONFIRMED_TICKS as u32 + 2), None);
}

#[test]
fn the_map_records_the_latest_confirmed_tick() {
    let mut map = ConfirmedTickMap::default();
    map.record(RepliconTick::new(9), NetTick(90));
    map.record(RepliconTick::new(10), NetTick(100));
    map.record(RepliconTick::new(11), NetTick(100));

    assert_eq!(
        map.record_last_confirmed_tick(RepliconTick::new(10)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(10),
            tick: NetTick(100),
        })
    );
    assert_eq!(
        map.record_last_confirmed_tick(RepliconTick::new(9)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(9),
            tick: NetTick(90),
        })
    );
    assert_eq!(
        map.record_last_confirmed_tick(RepliconTick::new(11)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(11),
            tick: NetTick(100),
        })
    );
    map.record_last_confirmed_tick(RepliconTick::new(10));
    assert_eq!(
        map.last_confirmed(),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(11),
            tick: NetTick(100),
        })
    );
    assert_eq!(map.last_confirmed_tick(), Some(NetTick(100)));
}

#[test]
fn the_map_finds_the_latest_completed_tick_before_a_limit() {
    let mut map = ConfirmedTickMap::default();
    let mut server_ticks = ServerMutateTicks::default();
    for (replicon, authoritative) in [(8, 80), (9, 90), (10, 100)] {
        let replicon_tick = RepliconTick::new(replicon);
        map.record(replicon_tick, NetTick(authoritative));
        server_ticks.confirm(replicon_tick, 1);
    }
    map.record_last_confirmed_tick(RepliconTick::new(10));

    assert_eq!(
        map.latest_completed_at_or_before(&server_ticks, NetTick(95)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(9),
            tick: NetTick(90),
        })
    );
    assert_eq!(
        map.latest_completed_at_or_before(&server_ticks, NetTick(100)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(10),
            tick: NetTick(100),
        })
    );
    assert_eq!(
        map.latest_completed_before(&server_ticks, NetTick(100)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(9),
            tick: NetTick(90),
        })
    );
}

#[test]
fn the_map_ignores_a_mapped_tick_that_is_not_complete() {
    let mut map = ConfirmedTickMap::default();
    let mut server_ticks = ServerMutateTicks::default();

    map.record(RepliconTick::new(8), NetTick(80));
    server_ticks.confirm(RepliconTick::new(8), 1);
    map.record_last_confirmed_tick(RepliconTick::new(8));

    // Userdata is recorded before Replicon applies the message. A mapping can therefore exist
    // before all mutation messages for its confirmed tick have arrived.
    map.record(RepliconTick::new(9), NetTick(90));

    // Put the cached frontier beyond the lookup bound so the historical lookup has to inspect
    // both the completed and incomplete retained mappings.
    map.record(RepliconTick::new(10), NetTick(110));
    server_ticks.confirm(RepliconTick::new(10), 1);
    map.record_last_confirmed_tick(RepliconTick::new(10));

    assert_eq!(
        map.latest_completed_at_or_before(&server_ticks, NetTick(100)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(8),
            tick: NetTick(80),
        })
    );
}

#[test]
fn the_map_takes_the_newest_replicon_tick_of_one_net_tick() {
    let mut map = ConfirmedTickMap::default();
    let mut server_ticks = ServerMutateTicks::default();
    for replicon in [8, 9] {
        let replicon_tick = RepliconTick::new(replicon);
        map.record(replicon_tick, NetTick(80));
        server_ticks.confirm(replicon_tick, 1);
    }
    map.record(RepliconTick::new(10), NetTick(100));
    server_ticks.confirm(RepliconTick::new(10), 1);
    map.record_last_confirmed_tick(RepliconTick::new(10));

    assert_eq!(
        map.latest_completed_at_or_before(&server_ticks, NetTick(80)),
        Some(ConfirmedTick {
            replicon_tick: RepliconTick::new(9),
            tick: NetTick(80),
        })
    );
}

#[test]
fn a_missing_confirmed_tick_leaves_the_cache() {
    let mut map = ConfirmedTickMap::default();

    assert_eq!(map.record_last_confirmed_tick(RepliconTick::new(10)), None);
    assert_eq!(map.last_confirmed_tick(), None);
    assert_eq!(map.last_confirmed(), None);
}

#[test]
fn a_clear_empties_the_confirmed_tick_cache() {
    let mut map = ConfirmedTickMap::default();
    map.record(RepliconTick::new(10), NetTick(100));
    map.record_last_confirmed_tick(RepliconTick::new(10));

    map.clear();

    assert_eq!(map.get(RepliconTick::new(10)), None);
    assert_eq!(map.last_confirmed_tick(), None);
    assert_eq!(map.last_confirmed(), None);
}

/// A completed tick or a message tick that no confirmed tick maps returns nothing and counts a
/// fault of its kind, which outlasts the link's confirmed ticks; a mapped message tick
/// counts none.
#[test]
fn an_unmapped_confirm_or_message_counts_a_fault() {
    let mut map = ConfirmedTickMap::default();
    map.record(RepliconTick::new(10), NetTick(100));

    assert_eq!(map.record_last_confirmed_tick(RepliconTick::new(11)), None);
    assert_eq!(map.record_last_confirmed_tick(RepliconTick::new(12)), None);
    assert_eq!(map.message_tick(RepliconTick::new(10)), Some(NetTick(100)));
    assert_eq!(map.message_tick(RepliconTick::new(9)), None);
    assert_eq!(map.message_tick(RepliconTick::new(13)), None);
    assert_eq!(map.message_tick(RepliconTick::new(14)), None);
    map.clear();

    assert_eq!(
        map.faults(),
        ConfirmedTickFaults {
            invalid_userdata: 0,
            unmapped_confirms: 2,
            unmapped_messages: 3,
        }
    );
}

/// The mutations arrive in any order, and the map keeps the newest window by tick. Replicon
/// ticks 1..=255 arrive, then 0 late, which fills the map; 256 then evicts 0, the oldest tick,
/// and not 1, the oldest arrival. A confirmation at 0 is then older than every retained
/// confirmed tick and resolves to the oldest retained tick, 10. Once the map is full, a late 0 is
/// not kept, and a late tick inside the window takes the place of the oldest one.
#[test]
fn a_late_confirmed_tick_takes_its_place_in_tick_order() {
    let mut map = ConfirmedTickMap::default();
    let max = MAX_CONFIRMED_TICKS as u32;
    for i in 1..max {
        map.record(RepliconTick::new(i), NetTick(i * 10));
    }
    map.record(RepliconTick::new(0), NetTick(0));
    assert_eq!(map.get(RepliconTick::new(0)), Some(NetTick(0)));

    map.record(RepliconTick::new(max + 10), NetTick((max + 10) * 10));
    assert_eq!(map.get(RepliconTick::new(0)), None);
    assert_eq!(map.get(RepliconTick::new(1)), Some(NetTick(10)));
    let confirmed = |map: &ConfirmedTickMap, tick| {
        map.confirmed_state_tick(&ConfirmHistory::new(RepliconTick::new(tick)))
    };
    assert_eq!(confirmed(&map, 0), Some(NetTick(10)));
    assert_eq!(confirmed(&map, max + 5), None);

    map.record(RepliconTick::new(0), NetTick(0));
    assert_eq!(map.get(RepliconTick::new(0)), None);

    map.record(RepliconTick::new(max + 5), NetTick((max + 5) * 10));
    assert_eq!(map.get(RepliconTick::new(1)), None);
    assert_eq!(map.get(RepliconTick::new(2)), Some(NetTick(20)));
    assert_eq!(confirmed(&map, max + 5), Some(NetTick((max + 5) * 10)));
    assert_eq!(confirmed(&map, 1), Some(NetTick(20)));
}

/// Replicon ticks wrap, and the window stays in send order across the wrap: `u32::MAX` is older
/// than 0.
#[test]
fn the_window_keeps_send_order_across_the_wrap() {
    let mut map = ConfirmedTickMap::default();
    map.record(RepliconTick::new(1), NetTick(30));
    map.record(RepliconTick::new(u32::MAX), NetTick(10));
    map.record(RepliconTick::new(0), NetTick(20));

    assert_eq!(
        map.confirmed_state_tick(&ConfirmHistory::new(RepliconTick::new(u32::MAX - 1))),
        Some(NetTick(10))
    );
    assert_eq!(map.get(RepliconTick::new(0)), Some(NetTick(20)));
    assert_eq!(map.get(RepliconTick::new(u32::MAX)), Some(NetTick(10)));
}
