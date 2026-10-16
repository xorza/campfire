use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::{FilterData, RelationSet};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

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
    pub gravity: Option<Num>,
}

/// What a projectile hits. In data: `none`, or a filter of the units it hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectileHits {
    /// No unit: it flies to its end, as Snow Owl flies to its point.
    Nothing,
    Units(FilterData),
}

/// Data is untrusted, so a speed that is not positive, a width or a range that is negative, or a
/// width whose half, the radius of the circle it flies as, is past `MAX_BOUND`, fails to read.
impl<'de> Deserialize<'de> for ProjectileData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            speed: Num,
            width: Option<Num>,
            range: Option<Num>,
            #[serde(default)]
            homing: bool,
            #[serde(default)]
            stop_on_hit: bool,
            #[serde(default)]
            once_per_cast: bool,
            hits: Option<ProjectileHits>,
            gravity: Option<Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        if fields.speed <= Num::ZERO {
            return Err(D::Error::custom("a projectile's speed is positive"));
        }
        let width = fields.width.unwrap_or(Num::ZERO);
        if width < Num::ZERO {
            return Err(D::Error::custom("a projectile's width is not negative"));
        }
        if width.to_bits() > 2 * MAX_BOUND.to_bits() {
            return Err(D::Error::custom("a projectile's width is at most 4,096 m"));
        }
        if fields.range.is_some_and(|range| range < Num::ZERO) {
            return Err(D::Error::custom("a projectile's range is not negative"));
        }
        Ok(Self {
            speed: fields.speed,
            width,
            range: fields.range,
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
    pub const ENEMIES: Self = Self::Units(FilterData {
        relations: RelationSet::Enemies,
        tags: Vec::new(),
    });
}

impl<'de> Deserialize<'de> for ProjectileHits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match text.as_str() {
            "none" => Ok(Self::Nothing),
            filter => FilterData::parse(filter)
                .map(ProjectileHits::Units)
                .ok_or_else(|| D::Error::custom(format!("filter {filter:?}"))),
        }
    }
}

impl DataSection for ProjectileData {
    const TABLE: DataTable = DataTable::Projectile;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::projectile_data::{ProjectileData, ProjectileHits};

    impl ProjectileData {
        /// A projectile of `speed` m/s and nothing more, as a section that names its speed alone
        /// reads: no width, its action's range, along a line, through what it hits.
        pub const fn flying(speed: Num) -> Self {
            Self {
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
