use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::filter_data::FilterData;
use crate::values::scalar::Scalar;

/// A unit type's `projectile` section, which makes its units projectiles: they fly `speed`
/// meters a second, `width` meters wide, for `range` meters or their action's range, homing on
/// a unit or along a line, and hit the units `hits` selects, `enemies` by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileData {
    pub speed: Num,
    pub width: Num,
    pub range: Option<Num>,
    pub homing: bool,
    /// It ends at its first hit.
    pub stop_on_hit: bool,
    /// A unit one projectile of a cast hit is no hit for another of the same cast.
    pub once_per_cast: bool,
    pub hits: Option<FilterData>,
    pub gravity: Option<Scalar>,
    pub sight_radius: Option<Scalar>,
    pub collide: Option<bool>,
}

/// Data is untrusted, so a speed that is not positive, or a width or a range that is negative,
/// fails to read.
impl<'de> Deserialize<'de> for ProjectileData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ProjectileData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            speed: Scalar,
            width: Option<Scalar>,
            range: Option<Scalar>,
            #[serde(default)]
            homing: bool,
            #[serde(default)]
            stop_on_hit: bool,
            #[serde(default)]
            once_per_cast: bool,
            hits: Option<FilterData>,
            gravity: Option<Scalar>,
            sight_radius: Option<Scalar>,
            collide: Option<bool>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let at_least = |value: Scalar, least: Num| value.to_num().filter(|&value| value >= least);
        let speed = fields
            .speed
            .to_num()
            .filter(|&speed| speed > Num::ZERO)
            .ok_or_else(|| D::Error::custom("a projectile's speed is positive"))?;
        let width = match fields.width {
            None => Num::ZERO,
            Some(width) => at_least(width, Num::ZERO)
                .ok_or_else(|| D::Error::custom("a projectile's width is not negative"))?,
        };
        let range = fields
            .range
            .map(|range| {
                at_least(range, Num::ZERO)
                    .ok_or_else(|| D::Error::custom("a projectile's range is not negative"))
            })
            .transpose()?;
        Ok(ProjectileData {
            speed,
            width,
            range,
            homing: fields.homing,
            stop_on_hit: fields.stop_on_hit,
            once_per_cast: fields.once_per_cast,
            hits: fields.hits,
            gravity: fields.gravity,
            sight_radius: fields.sight_radius,
            collide: fields.collide,
        })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::projectiles::projectile_data::ProjectileData;

    impl ProjectileData {
        /// A projectile of `speed` m/s and nothing more, as a section that names its speed alone
        /// reads: no width, its action's range, along a line, through what it hits.
        pub(crate) const fn flying(speed: Num) -> ProjectileData {
            ProjectileData {
                speed,
                width: Num::ZERO,
                range: None,
                homing: false,
                stop_on_hit: false,
                once_per_cast: false,
                hits: None,
                gravity: None,
                sight_radius: None,
                collide: None,
            }
        }
    }
}
