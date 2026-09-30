//! The lane mode's waves: two waves fight to an end within the time between waves, and the
//! winner walks into the enemy tower's reach and falls to it; a whole wave there strikes the tower
//! before it falls.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, Health, MoveStep, Owner, Team};
use campfire_log::LogLine;
use campfire_log::internals::capture;
use campfire_math::Num;
use campfire_net::{LocalMatch, MatchSetup, UnitDied};
use campfire_protocol::SeedChain;
use campfire_sim::{EntityIndex, Position, StableId};
use lightyear::prelude::RollbackMode;

use crate::scenario::next_tick;

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);

/// A match of the lane mode whose one player's walker stands at the map's edge, 8 m off the lane,
/// out of every creep's and tower's reach, from before the first wave.
fn quiet_lane() -> LocalMatch {
    let mut local = LocalMatch::new(MatchSetup::solo(RollbackMode::Check, 1, SEED_CHAIN));
    local.start_match();
    local.order(
        0,
        Action::Move {
            x: Num::ZERO,
            z: Num::from_int(8).unwrap(),
        },
    );
    local
}

/// The creeps of `team`, and the tower of `team`: the units that walk and have no owner, and the
/// one that neither walks nor has an owner.
fn creeps(app: &App, team: u8) -> Vec<Entity> {
    units(app, team, true)
}

fn tower(app: &App, team: u8) -> Entity {
    units(app, team, false)[0]
}

fn units(app: &App, team: u8, walks: bool) -> Vec<Entity> {
    let world = app.world();
    world
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, entity)| entity)
        .filter(|&entity| {
            let unit = world.entity(entity);
            unit.get::<Team>() == Some(&Team::new(team))
                && unit.contains::<MoveStep>() == walks
                && !unit.contains::<Owner>()
        })
        .collect()
}

fn position(app: &App, unit: Entity) -> Position {
    *app.world().get::<Position>(unit).unwrap()
}

/// The stable ids of `units`, in order.
fn ids(app: &App, units: &[Entity]) -> Vec<StableId> {
    let mut ids: Vec<_> = units
        .iter()
        .map(|&unit| *app.world().get::<StableId>(unit).unwrap())
        .collect();
    ids.sort_unstable();
    ids
}

/// The deaths the server logged in `lines`, in order.
pub(crate) fn deaths_logged(lines: &[String]) -> Vec<UnitDied> {
    lines
        .iter()
        .filter_map(|line| LogLine::parse(line).unwrap().read::<UnitDied>())
        .map(Result::unwrap)
        .collect()
}

fn health(app: &App, unit: Entity) -> Num {
    app.world().get::<Health>(unit).unwrap().current()
}

/// 30 s between waves at 30 ticks a second.
const WAVE_TICKS: u64 = 900;

#[test]
fn two_waves_fight_to_an_end_and_the_enemy_tower_kills_the_winner() {
    // The waves spawn 32 m apart and close at 7.5 m/s: they meet about 4 s later. Two creeps
    // kill one of 300 health with twelve strikes of 25, six each: the sixth strikes 9 + 5 × 24 =
    // 129 ticks into their fight, 4.3 s. So a wave falls well within 15 s, half the time between
    // waves, and the fight leaves one wave standing.
    let mut local = quiet_lane();
    while creeps(local.server(), 0).is_empty() {
        local.step();
    }
    let spawned = next_tick(local.server());
    let left = |local: &LocalMatch| [0, 1].map(|team| creeps(local.server(), team).len());
    while left(&local).iter().all(|&count| count > 0) {
        local.step();
    }
    assert!(next_tick(local.server()) - spawned < WAVE_TICKS / 2);
    let winner = u8::from(left(&local)[0] == 0);
    assert_eq!(left(&local)[usize::from(1 - winner)], 0);

    // The winner walks on into the enemy tower's reach, 7.75 m, and the tower kills it there
    // before the next wave.
    let enemy_tower = tower(local.server(), 1 - winner);
    let tower_at = position(local.server(), enemy_tower);
    let survivor = creeps(local.server(), winner)[0];
    let mut last = position(local.server(), survivor);
    while local.server().world().get_entity(survivor).is_ok() {
        assert!(
            next_tick(local.server()) - spawned < WAVE_TICKS,
            "the tower kills the winner"
        );
        last = position(local.server(), survivor);
        local.step();
    }
    let reach = Num::from_bits(31 << (Num::FRAC_BITS - 2));
    assert!(tower_at.within_ground(last, reach), "{last:?}");
}

#[test]
fn a_wave_with_no_wave_to_meet_strikes_the_tower_and_falls_to_it() {
    // The west wave goes as it spawns. The east wave walks into the west tower's reach, strikes
    // it for 25 a strike, and falls to it: two of its strikes of 150 kill a creep of 300. All
    // this before the next wave.
    let mut local = quiet_lane();
    while creeps(local.server(), 0).is_empty() {
        local.step();
    }
    let spawned = next_tick(local.server());
    for creep in creeps(local.server(), 0) {
        local.server_mut().world_mut().despawn(creep);
    }
    let west = tower(local.server(), 0);
    let east_wave = ids(local.server(), &creeps(local.server(), 1));
    let lines = capture(|| {
        while !creeps(local.server(), 1).is_empty() {
            assert!(
                next_tick(local.server()) - spawned < WAVE_TICKS,
                "the tower kills the wave"
            );
            local.step();
        }
    });
    // The server logs each creep's death once, with its team and the tower as killer, though the
    // creep is gone by then.
    let west_id = *local.server().world().get::<StableId>(west).unwrap();
    let mut died: Vec<_> = deaths_logged(&lines)
        .into_iter()
        .map(|death| (death.unit, death.team, death.owner, death.killer))
        .collect();
    died.sort_unstable_by_key(|&(unit, ..)| unit);
    let expected: Vec<_> = east_wave
        .into_iter()
        .map(|unit| (unit, Some(Team::new(1)), None, Some(west_id)))
        .collect();
    assert_eq!(died, expected);
    let full = Num::from_int(1500).unwrap();
    let lost = full - health(local.server(), west);
    let strike = Num::from_int(25).unwrap();
    assert!(lost > Num::ZERO && lost < full, "{lost:?}");
    assert_eq!(lost.to_bits() % strike.to_bits(), 0, "{lost:?}");
}
