use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::filter_data::FilterData;
use crate::values::relation_set::RelationSet;
use crate::values::scalar::Scalar;

/// A unit type's `projectile` section, which makes its units projectiles: they fly `speed`
/// meters a second, `width` meters wide, for `range` meters or their action's range, homing on
/// a unit or along a line, and hit what `hits` says, the units of `enemies` by default.
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
    pub hits: ProjectileHits,
    pub gravity: Option<Scalar>,
}

/// What a projectile hits. In data: `none`, or a filter of the units it hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectileHits {
    /// No unit: it flies to its end, as Snow Owl flies to its point.
    Nothing,
    Units(FilterData),
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
            hits: Option<ProjectileHits>,
            gravity: Option<Scalar>,
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
            hits: fields.hits.unwrap_or(ProjectileHits::ENEMIES),
            gravity: fields.gravity,
        })
    }
}

impl ProjectileHits {
    /// The units of `enemies`, which a projectile hits when its data names none.
    pub const ENEMIES: ProjectileHits = ProjectileHits::Units(FilterData {
        relations: RelationSet::Enemies,
        tags: Vec::new(),
    });
}

impl<'de> Deserialize<'de> for ProjectileHits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ProjectileHits, D::Error> {
        let text = String::deserialize(deserializer)?;
        match text.as_str() {
            "none" => Ok(ProjectileHits::Nothing),
            filter => FilterData::parse(filter)
                .map(ProjectileHits::Units)
                .ok_or_else(|| D::Error::custom(format!("filter {filter:?}"))),
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::projectiles::projectile_data::{ProjectileData, ProjectileHits};

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
                hits: ProjectileHits::ENEMIES,
                gravity: None,
            }
        }
    }
}
