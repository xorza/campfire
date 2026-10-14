use std::num::NonZeroI64;

use campfire_declare::PerType;
use campfire_math::Num;
use campfire_script::{Filter, UnknownName};
use campfire_sim::{Capability, TickRate};
use campfire_units::UnitTypeBook;

use crate::projectile_data::{ProjectileData, ProjectileHits};

/// A projectile type as a match runs it: its speed a tick, its width, its range if it has its
/// own, whether it homes, stops at its first hit, or hits a unit once a cast, and the units it
/// hits, none when it hits nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectileSpec {
    pub(crate) speed: Num,
    pub(crate) width: Num,
    pub(crate) range: Option<Num>,
    pub homing: bool,
    pub(crate) stop_on_hit: bool,
    pub(crate) once_per_cast: bool,
    pub(crate) hits: Option<Filter>,
}

impl ProjectileSpec {
    /// The spec of `data`: its speed a tick at `rate`, and what it hits among the tags of
    /// `types`; an error for a filter of a tag `types` lacks.
    pub fn of(
        data: &ProjectileData,
        types: &UnitTypeBook,
        rate: TickRate,
    ) -> Result<ProjectileSpec, UnknownName> {
        let hits = match &data.hits {
            ProjectileHits::Nothing => None,
            ProjectileHits::Units(filter) => Some(Filter::resolve(filter, types.tags())?),
        };
        Ok(ProjectileSpec {
            speed: data
                .speed
                .checked_div_int(NonZeroI64::from(rate.hz()).get())
                .expect("a speed over a tick rate fits"),
            width: data.width,
            range: data.range,
            homing: data.homing,
            stop_on_hit: data.stop_on_hit,
            once_per_cast: data.once_per_cast,
            hits,
        })
    }
}

impl PerType for ProjectileSpec {
    const OWNER: Option<Capability> = Some(Capability::Projectiles);
    const SCRIPTS_ONLY: bool = false;
}
