use campfire_combat::{CombatData, OnDeath};
use campfire_core::{MoveStep, UnitType};
use campfire_sim::TickRate;
use campfire_stats::{Life, Meter, PoolId, Pools, StatBook};
use campfire_values::{EngineStat, Stat};

use crate::books::error::UnitStartError;

/// What a new unit of a type starts with of its stats, in ticks at the match's rate: its life and
/// its other pools, full at their maxima at level 1, whether it stays when it dies, when it has a
/// `combat` section, and how far it walks a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitStart {
    pub(crate) life: Option<Life>,
    pub(crate) pools: Option<Pools>,
    pub(crate) on_death: Option<OnDeath>,
    pub(crate) step: Option<MoveStep>,
}

/// What a unit type's start is made of: the mode's stats, `book`, its life pool, `life`, which a
/// mode with no combat lacks, and the match's `rate`; then the type, `unit_type`, its `combat`
/// section, and its `pools`, each with the stat of its maximum.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StartOf<'a> {
    pub(crate) book: &'a StatBook,
    pub(crate) life: Option<PoolId>,
    pub(crate) rate: TickRate,
    pub(crate) unit_type: UnitType,
    pub(crate) combat: Option<&'a CombatData>,
    pub(crate) pools: &'a [(PoolId, &'a Stat)],
}

impl UnitStart {
    /// The start `of` makes. Each pool's maximum is the stat of its maximum at level 1, and the
    /// move step is the book's at level 1 at the rate, as a refresh computes them. A type has
    /// `combat` exactly when it has the life pool: a unit that can die is one that combat kills.
    pub(crate) fn new(of: StartOf<'_>) -> Result<UnitStart, UnitStartError> {
        let StartOf {
            book,
            life,
            rate,
            unit_type,
            combat,
            pools,
        } = of;
        let values = book.base_values(unit_type, 1);
        let given = |stat: &Stat| {
            let id = book
                .named(stat)
                .expect("the pool book resolved each pool's stats");
            book.gives(unit_type, id).then_some(id)
        };
        let mut maxes = Vec::new();
        let mut full_life = None;
        for &(pool, max) in pools {
            let id = given(max).ok_or_else(|| UnitStartError::MissingStat(max.clone()))?;
            let value = values[id.index()];
            let meter =
                Meter::new(value).ok_or_else(|| UnitStartError::NotPositive(max.clone()))?;
            if Some(pool) == life {
                full_life = Some(Life::new(meter));
            } else {
                maxes.push((pool, value));
            }
        }
        let pools =
            (!maxes.is_empty()).then(|| Pools::new(maxes).expect("each maximum is positive"));
        let on_death = match (combat, full_life.is_some()) {
            (Some(combat), true) => Some(combat.on_death),
            (Some(_), false) => return Err(UnitStartError::NoLifePool),
            (None, true) => return Err(UnitStartError::NoCombat),
            (None, false) => None,
        };
        // A mode with no move speed among its stats moves no unit.
        let moves = book
            .named(&Stat::Engine(EngineStat::MoveSpeed))
            .is_some_and(|id| book.gives(unit_type, id));
        let step = moves.then(|| {
            let step = book
                .step(&values, rate)
                .expect("a type gives only a stat the mode declares");
            MoveStep::new(step).expect("the book's step is never negative")
        });
        Ok(UnitStart {
            life: full_life,
            pools,
            on_death,
            step,
        })
    }
}

#[cfg(test)]
mod tests;
