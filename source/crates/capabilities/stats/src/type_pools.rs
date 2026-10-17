use campfire_declare::{BookCx, BookError, BookPlace, ByType, Fact, NamesListError, SourceType};
use campfire_math::Num;
use campfire_script::{NameKind, UnknownName};
use campfire_units::{Life, Meter};
use campfire_values::{NameId, Stat};

use crate::error::UnitStartError;
use crate::life_pool::LifePool;
use crate::mode_pools_data::ModePoolsData;
use crate::pool_names::PoolNames;
use crate::pools::Pools;
use crate::stat_book::StatBook;
use crate::type_pools_data::TypePoolsData;

/// What each unit type that stands starts with of its pools, full at their maxima at level 1: its
/// life, of the mode's life pool, and its other pools.
#[derive(Debug, Default)]
pub struct TypePools {
    pub lives: ByType<Life>,
    pub pools: ByType<Pools>,
}

impl Fact for TypePools {}

impl TypePools {
    /// The rule of every unit type's pools: none twice.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for unit in cx.types() {
            let Some(pools) = unit.data.section::<TypePoolsData>() else {
                continue;
            };
            let repeated =
                (pools.0.iter().enumerate()).find(|&(at, name)| pools.0[..at].contains(name));
            if let Some((_, name)) = repeated {
                let at = BookPlace::UnitType(unit.place());
                let problem = NamesListError::Repeated(name.clone());
                return Err(BookError::rule(unit.package, at, problem));
            }
        }
        Ok(())
    }

    /// The pools of each unit type that stands; an error for a pool the mode does not declare,
    /// or for a maximum the type's stats do not give or give no positive value of.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let stats = cx.book::<StatBook>()?;
        let names = &cx.fact::<PoolNames>()?.0;
        let data = cx.mode_section::<ModePoolsData>();
        let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
        let mut built = Self::default();
        for unit in cx.standing() {
            let Some(pools) = unit.data.section::<TypePoolsData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let mut maxes = Vec::new();
            let mut full_life = None;
            let values = stats.base_values(unit.unit_type, 1);
            for name in &pools.0 {
                let pool = names.named(name.as_str());
                let unknown = || UnknownName::new(NameKind::Pool, name);
                let pool = pool.ok_or_else(|| BookError::unknown(unit.package, at(), unknown()))?;
                let max = &data.expect("the mode declares its pools").0[name].max;
                let meter = Self::max(stats, unit, &values, max)?;
                if Some(pool) == life {
                    full_life = Some(Life::new(meter));
                } else {
                    maxes.push((pool, meter.max()));
                }
            }
            built.lives.set_some(unit.unit_type, full_life);
            let pools =
                (!maxes.is_empty()).then(|| Pools::new(maxes).expect("each maximum is positive"));
            built.pools.set_some(unit.unit_type, pools);
        }
        Ok(built)
    }

    /// The life each unit type of the life pool starts with.
    pub fn lives(cx: &BookCx<'_>) -> Result<ByType<Life>, BookError> {
        Ok(cx.fact::<Self>()?.lives.clone())
    }

    /// The pools each unit type that stands starts with but its life.
    pub fn pools(cx: &BookCx<'_>) -> Result<ByType<Pools>, BookError> {
        Ok(cx.fact::<Self>()?.pools.clone())
    }

    /// The full meter of a pool of `unit` whose maximum is `max`, of `values`, its stats' at
    /// level 1; an error for a maximum it gives no value or no positive value of.
    fn max(
        stats: &StatBook,
        unit: &SourceType<'_>,
        values: &[Num],
        max: &Stat,
    ) -> Result<Meter, BookError> {
        let fail = |error| BookError::rule(unit.package, BookPlace::UnitType(unit.place()), error);
        let id = stats
            .named(max)
            .expect("the pool book resolves each pool's stats");
        if !stats.gives(unit.unit_type, id) {
            return Err(fail(UnitStartError::MissingStat(max.clone())));
        }
        Meter::new(values[id.index()]).ok_or_else(|| fail(UnitStartError::NotPositive(max.clone())))
    }
}
