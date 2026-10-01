//! The lane mode's waves: two equal waves trade to an end within the time between waves, and a
//! whole wave with no wave to meet walks into the enemy tower's reach, strikes the tower, and falls
//! to it.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, MoveStep, Owner, PoolId, Pools, Team};
use campfire_math::Num;
use campfire_net::{LocalMatch, MatchSetup};
use campfire_protocol::SeedChain;
use campfire_sim::EntityIndex;
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

/// The life pool of `unit`: the lane mode's `health`, the first of its pools by name.
fn health(app: &App, unit: Entity) -> Num {
    let pools = app.world().get::<Pools>(unit).unwrap();
    pools.current(PoolId::FIRST).unwrap()
}

/// 30 s between waves at 30 ticks a second.
const WAVE_TICKS: u64 = 900;

#[test]
fn two_equal_waves_trade_to_an_end_before_the_next_wave() {
    // The waves spawn 32 m apart and close at 7.5 m/s: they meet about 4 s later. The lane is a
    // mirror, so each creep has its twin: the two front creeps fall to twelve strikes of 25, six
    // from each rear creep, then the two rear creeps to twelve more, 24 ticks apart, 9.6 s. All
    // four fall well before the next wave, the twins in the same tick, and neither wave is left
    // to push: a hero tips the balance.
    let mut local = quiet_lane();
    while creeps(local.server(), 0).is_empty() {
        local.step();
    }
    let spawned = next_tick(local.server());
    let mut gone = [None; 2];
    while gone.iter().any(Option::is_none) {
        assert!(
            next_tick(local.server()) - spawned < WAVE_TICKS,
            "the fight ends"
        );
        local.step();
        for (team, gone) in (0..).zip(&mut gone) {
            if gone.is_none() && creeps(local.server(), team).is_empty() {
                *gone = Some(next_tick(local.server()));
            }
        }
    }
    assert_eq!(gone[0], gone[1]);
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
    while !creeps(local.server(), 1).is_empty() {
        assert!(
            next_tick(local.server()) - spawned < WAVE_TICKS,
            "the tower kills the wave"
        );
        local.step();
    }
    let full = Num::from_int(1500).unwrap();
    let lost = full - health(local.server(), west);
    let strike = Num::from_int(25).unwrap();
    assert!(lost > Num::ZERO && lost < full, "{lost:?}");
    assert_eq!(lost.to_bits() % strike.to_bits(), 0, "{lost:?}");
}
