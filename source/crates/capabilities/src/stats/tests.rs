use std::collections::BTreeMap;
use std::mem;
use std::num::NonZeroU32;

use bevy_ecs::entity::Entity;
use campfire_sim::{Capability, SimUpdate, TickRate, Ticks};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::stats::stat::Stat;
use crate::stats::stat_rule::{Combine, StatRule};
use crate::stats::stats_data::{StatValue, StatsData};
use crate::values::scalar::Scalar;

/// 30 ticks a second, as the MOBA runs.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// `value` sixteenths.
fn sixteenths(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 4))
}

fn rule(combine: Combine, min: Option<Num>, max: Option<Num>) -> StatRule {
    StatRule { combine, min, max }
}

/// The rules of the test: the engine's stats, move speed at most 5, its rate at least −1, the
/// slow the highest from 0 to 0.99.
fn rules() -> BTreeMap<Stat, StatRule> {
    let sum = rule(Combine::Sum, None, None);
    let slow_max = Num::from_bits((99 << Num::FRAC_BITS) / 100);
    [
        (EngineStat::Health, sum),
        (EngineStat::HealthRegen, sum),
        (EngineStat::Resource, sum),
        (
            EngineStat::MoveSpeed,
            rule(Combine::Sum, None, Some(num(5))),
        ),
        (
            EngineStat::MoveSpeedPct,
            rule(Combine::Sum, Some(num(-1)), None),
        ),
        (
            EngineStat::Slow,
            rule(Combine::Highest, Some(Num::ZERO), Some(slow_max)),
        ),
        (EngineStat::AttackSpeed, sum),
        (EngineStat::AttackDamage, sum),
    ]
    .map(|(stat, rule)| (Stat::Engine(stat), rule))
    .into()
}

/// A unit type's stats, each a base and a gain a level.
fn stats(values: &[(EngineStat, Num, Num)]) -> StatsData {
    StatsData(
        values
            .iter()
            .map(|&(stat, base, per_level)| {
                let value = StatValue {
                    base: Scalar::Decimal(base),
                    per_level: Some(Scalar::Decimal(per_level)),
                };
                (Stat::Engine(stat), value)
            })
            .collect(),
    )
}

/// A match with the stats capability and a book of `types`, with a move speed cap of 6.
fn stat_match(types: &[StatsData]) -> TestMatch {
    let mut game = TestMatch::new(&[Capability::Stats], RATE, None);
    let types = types
        .iter()
        .enumerate()
        .map(|(at, data)| (UnitType::new(u16::try_from(at).unwrap()), data));
    let book = StatBook::new(&rules(), types, RATE, num(6)).unwrap();
    Stats::load(&mut game.world, book);
    game.world.add_schedule(mem::take(&mut game.schedule));
    game
}

/// A unit of `unit_type` at level 1 that walks, attacks with a windup of 2 ticks, and has a
/// pool of health and of resource, its values before the stats derive them.
fn unit(game: &mut TestMatch, unit_type: u16) -> Entity {
    let attack = AttackStats::new(Num::ONE, Ticks::new(2), Ticks::new(3), Num::ZERO).unwrap();
    game.world
        .spawn((
            UnitType::new(unit_type),
            Level::default(),
            UnitStats::default(),
            MoveStep::new(Num::ZERO).unwrap(),
            attack,
            Health::new(Num::ONE).unwrap(),
            ResourcePool::new(Num::ONE).unwrap(),
        ))
        .id()
}

#[test]
fn a_units_stats_follow_its_type_and_level_within_their_limits() {
    // A hero of health 380 + 76 a level, regen 1.5, resource 250 + 45, attack damage 51 + 3,
    // attack speed 0.625 + 0.0625 and move speed 4 + 0.25.
    let hero = stats(&[
        (EngineStat::Health, num(380), num(76)),
        (EngineStat::HealthRegen, sixteenths(24), Num::ZERO),
        (EngineStat::Resource, num(250), num(45)),
        (EngineStat::AttackDamage, num(51), num(3)),
        (EngineStat::AttackSpeed, sixteenths(10), sixteenths(1)),
        (EngineStat::MoveSpeed, num(4), sixteenths(4)),
    ]);
    // A slow of 0.5 on move speed 4, and one of 1.5 the limit cuts to 0.99; a move speed rate
    // of −2 the limit raises to −1.
    let slowed = stats(&[
        (EngineStat::MoveSpeed, num(4), Num::ZERO),
        (EngineStat::Slow, sixteenths(8), Num::ZERO),
    ]);
    let stopped = stats(&[
        (EngineStat::MoveSpeed, num(4), Num::ZERO),
        (EngineStat::MoveSpeedPct, num(-2), Num::ZERO),
    ]);
    let mut game = stat_match(&[hero, slowed, stopped]);
    let units = [0, 1, 2].map(|unit_type| unit(&mut game, unit_type));
    game.world.run_schedule(SimUpdate);
    let get = |game: &TestMatch| {
        let world = &game.world;
        let unit = world.entity(units[0]);
        (
            unit.get::<MoveStep>().unwrap().get(),
            unit.get::<AttackStats>().unwrap().period(),
            unit.get::<AttackStats>().unwrap().damage(),
            *unit.get::<Health>().unwrap(),
            unit.get::<ResourcePool>().unwrap().max(),
        )
    };
    // At level 1: 4 m/s, 4 × 2²⁴ ÷ 30 = 2 236 962.13 bits a tick, to 2 236 962; 0.625 attacks a
    // second, 30 ÷ 0.625 = 48 ticks; damage 51; full health of 380 and resource of 250.
    let (step, period, damage, health, resource) = get(&game);
    assert_eq!(step, Num::from_bits(2_236_962));
    assert_eq!((period, damage), (Ticks::new(48), num(51)));
    assert_eq!(
        (health.current(), health.max(), resource),
        (num(380), num(380), num(250))
    );
    // The slowed one walks 4 × 0.5 = 2 m/s, 2 × 2²⁴ ÷ 30 = 1 118 481.07 bits, to 1 118 481; the
    // stopped one, at a rate of −1, not at all.
    let step_of = |game: &TestMatch, unit| game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step_of(&game, units[1]), Num::from_bits(1_118_481));
    assert_eq!(step_of(&game, units[2]), Num::ZERO);

    // Down 80 to 300 of 380, then at level 18: health 380 + 76 × 17 = 1672, the pool up by the
    // 1292 the maximum rose, to 1592, plus the regen of the tick that runs, 1.5 ÷ 30 = 0.05,
    // 838 860.8 bits, to 838 860 and its fifth carried; attack speed 0.625 + 17 × 0.0625 =
    // 1.6875, 30 ÷ 1.6875 = 17.8 ticks, to 18; damage 51 + 51 = 102; move speed 4 + 4.25 = 8.25,
    // past the limit of 5, 5 × 2²⁴ ÷ 30 = 2 796 202.67, to 2 796 203.
    let mut health = game.world.get_mut::<Health>(units[0]).unwrap();
    health.take(num(80));
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::new(18).unwrap();
    game.world.run_schedule(SimUpdate);
    let (step, period, damage, health, resource) = get(&game);
    assert_eq!(step, Num::from_bits(2_796_203));
    assert_eq!((period, damage), (Ticks::new(18), num(102)));
    let regen = Num::from_bits(838_860);
    assert_eq!(
        (health.current(), health.max()),
        (num(1592) + regen, num(1672))
    );
    assert_eq!(resource, num(250 + 45 * 17));

    // Over the next 29 ticks the regen adds the rest of the second: 30 ticks gain exactly 1.5.
    for _ in 0..29 {
        game.world.run_schedule(SimUpdate);
    }
    let health = game.world.get::<Health>(units[0]).unwrap();
    assert_eq!(health.current(), num(1592) + sixteenths(24));

    // Back to level 1: the maximum falls to 380, and the current amount to it.
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::default();
    game.world.run_schedule(SimUpdate);
    let (_, _, _, health, _) = get(&game);
    assert_eq!((health.current(), health.max()), (num(380), num(380)));
}
