//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::{Added, Changed, Or, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_math::Num;
use campfire_sim::{SimSet, StateRegistry};

use crate::abilities::resource_pool::ResourcePool;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::navigation::move_step::MoveStep;
use crate::stats::level::Level;
use crate::stats::stat::EngineStat;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::script_view::{RowFill, View};
use crate::units::unit_type::UnitType;

pub(crate) mod level;
pub(crate) mod modifier_data;
pub(crate) mod stat;
pub(crate) mod stat_book;
pub(crate) mod stat_rule;
pub(crate) mod stats_data;
pub(crate) mod unit_state;
pub(crate) mod unit_stats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

impl Stats {
    /// Adds stats to a match: before the first stage and after each, every unit whose level
    /// changed, or that is new, has its stats derived again, and the components that hold their
    /// effect follow them; as each tick starts, the living units' pools regenerate. With no stat
    /// book, as before a mode loads one or on a client, which loads none, nothing changes.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        registry.register_component::<Level>();
        schedule.add_systems((
            regenerate.in_set(SimSet::Inputs),
            refresh_stats.before(SimSet::Inputs),
        ));
        for pair in SimSet::ALL.windows(2) {
            schedule.add_systems(refresh_stats.after(pair[0]).before(pair[1]));
        }
        schedule.add_systems(refresh_stats.after(SimSet::Vision));
    }

    /// Gives the match the mode's stat book.
    pub(crate) fn load(world: &mut World, book: StatBook) {
        world.insert_resource(book);
    }
}

/// Fills a row of the script view with the unit's level and stats.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.level = unit.get::<Level>().map(|level| level.get());
    if let Some(stats) = unit.get::<UnitStats>() {
        fill.stated(stats.values());
    }
}

/// Derives the stats of each unit whose level changed or that is new, and sets what holds their
/// effect: how far it walks a tick, its attack's damage and period, and its pools' maxima, a
/// pool keeping the rule of stats.md. An effect whose stat the mode does not declare keeps what
/// the unit's kit gave it.
fn refresh_stats(
    book: Option<Res<'_, StatBook>>,
    mut units: Query<
        '_,
        '_,
        (
            &UnitType,
            &Level,
            &mut UnitStats,
            Option<&mut MoveStep>,
            Option<&mut AttackStats>,
            Option<&mut Health>,
            Option<&mut ResourcePool>,
        ),
        Or<(Changed<Level>, Added<UnitStats>)>,
    >,
) {
    let Some(book) = book else {
        return;
    };
    for (&unit_type, level, mut stats, step, attack, health, pool) in &mut units {
        book.compute(unit_type, level.get(), stats.refill());
        let values = stats.values();
        if let (Some(mut step), Some(value)) = (step, book.step(values)) {
            step.set_if_neq(MoveStep::new(value).expect("a step is at least 0"));
        }
        if let Some(mut attack) = attack {
            let damage = book.engine(values, EngineStat::AttackDamage);
            let period = book.period(values, attack.windup());
            if let (Some(damage), Some(period)) = (damage, period) {
                let derived = attack.derived(damage.max(Num::ZERO), period);
                attack.set_if_neq(derived);
            }
        }
        let positive = |value: Num| value.max(Num::EPSILON);
        if let (Some(mut health), Some(max)) = (health, book.engine(values, EngineStat::Health)) {
            let mut changed = *health;
            changed.set_max(positive(max));
            health.set_if_neq(changed);
        }
        if let (Some(mut pool), Some(max)) = (pool, book.engine(values, EngineStat::Resource)) {
            let mut changed = *pool;
            changed.set_max(positive(max));
            pool.set_if_neq(changed);
        }
    }
}

/// Adds each living unit's regen to its pools: `health_regen` and `resource_regen` a second,
/// the tick rate's share a tick, the remainder carried so a second gains exactly the regen.
fn regenerate(
    book: Option<Res<'_, StatBook>>,
    mut units: Query<
        '_,
        '_,
        (&UnitStats, Option<&mut Health>, Option<&mut ResourcePool>),
        Without<Dead>,
    >,
) {
    let Some(book) = book else {
        return;
    };
    let hz = book.rate().hz().get();
    for (stats, health, pool) in &mut units {
        let values = stats.values();
        if let (Some(mut health), Some(regen)) =
            (health, book.engine(values, EngineStat::HealthRegen))
        {
            let mut changed = *health;
            changed.regen(regen, hz);
            health.set_if_neq(changed);
        }
        if let (Some(mut pool), Some(regen)) =
            (pool, book.engine(values, EngineStat::ResourceRegen))
        {
            let mut changed = *pool;
            changed.regen(regen, hz);
            pool.set_if_neq(changed);
        }
    }
}

#[cfg(test)]
mod tests;
