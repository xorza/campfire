use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_script::MatchNames;
use campfire_units::{PoolId, StatId};
use campfire_values::{Names, PackageIndex, Stat};

use crate::life_pool::LifePool;
use crate::mode_pools_data::ModePoolsData;
use crate::pool_data::PoolData;
use crate::stat_book::StatBook;

/// The pools the mode declares, by pool id: the place among the stats of each one's maximum and
/// of its regen. Package data, not state.
#[derive(Resource, Debug)]
#[component(immutable)]
pub struct PoolBook {
    /// The pools' names, by id; shared with the script view, which names them to scripts.
    names: Arc<Names<PoolId>>,
    pools: Vec<PoolStats>,
    /// The life pool, which the mode's `[combat]` names: a unit with it lives while it holds more
    /// than nothing, and is a target. None in a mode that names none, where no unit is a target.
    life: Option<PoolId>,
}

/// The places among the stats of a pool's maximum and of its regen a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    pub(crate) max: StatId,
    pub(crate) regen: Option<StatId>,
}

impl PoolBook {
    /// The book of `pools`, named `names`, the life pool `life` among them, each pool's stats
    /// resolved by `stat`, given the pool; the first error `stat` gives.
    pub fn new<E>(
        pools: &BTreeMap<DeclaredName, PoolData>,
        names: Arc<Names<PoolId>>,
        life: Option<PoolId>,
        stat: impl Fn(&DeclaredName, &Stat) -> Result<StatId, E>,
    ) -> Result<Self, E> {
        assert_eq!(names.len(), pools.len(), "a pool has a name");
        let pools = pools.iter().map(|(name, pool)| {
            Ok(PoolStats {
                max: stat(name, &pool.max)?,
                regen: pool
                    .regen
                    .as_ref()
                    .map(|regen| stat(name, regen))
                    .transpose()?,
            })
        });
        Ok(Self {
            names,
            pools: pools.collect::<Result<_, E>>()?,
            life,
        })
    }

    /// The life pool; none in a mode that names none.
    pub const fn life(&self) -> Option<PoolId> {
        self.life
    }

    /// Whether a pool regenerates, by a stat of its regen.
    pub(crate) fn regenerates(&self) -> bool {
        self.pools.iter().any(|stats| stats.regen.is_some())
    }

    /// Each pool, with its stats.
    pub fn iter(&self) -> impl Iterator<Item = (PoolId, PoolStats)> + '_ {
        (0..).zip(&self.pools).map(|(at, &stats)| {
            let pool = PoolId::new(at).expect("the mode's names hold its pools within the limit");
            (pool, stats)
        })
    }
}

impl PoolBook {
    /// The mode's pools, each with its stats, and its life pool, as the module that kills units
    /// names it; an error for a stat the mode does not declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let none = BTreeMap::new();
        let pools = cx.mode_section::<ModePoolsData>();
        let pools = pools.map_or(&none, |pools| &pools.0);
        let names = Arc::clone(cx.names::<PoolId>()?);
        let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
        let stats = cx.book::<StatBook>()?;
        Self::new(pools, names, life, |pool, stat| {
            let at = BookPlace::Entry {
                table: ModePoolsData::KEY,
                name: pool.clone(),
            };
            stats.named_in(PackageIndex::MODE, at, stat)
        })
    }
}

impl Book for PoolBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(&self.names));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_units::PoolId;

    use crate::pool_book::PoolBook;

    impl PoolBook {
        /// Makes `life` the life pool, as a test world with no mode needs.
        pub const fn set_life(&mut self, life: Option<PoolId>) {
            self.life = life;
        }
    }
}
