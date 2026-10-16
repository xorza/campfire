use campfire_actions::ProjectileFlight;
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::{FilterData, RelationSet};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A unit type's `projectile` section, which makes its units projectiles: they fly `speed`
/// meters a second, `width` meters wide, for `range` meters or their action's range, homing on
/// a unit, along a line or on a `curve`, and hit what `hits` says, the units of `enemies` by
/// default.
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
    pub curve: Option<CurveData>,
}

/// A projectile's `curve`, a cubic Bézier curve from its launch to its target
/// (04-capabilities/actions.md, A projectile may fly a curve): its inner points at `first`'s and
/// `second`'s fractions of the way, `[fraction, height]` each, their heights above the highest
/// of its ends and the ground between them; its end following its target by `follow` meters a
/// second; and its life, `life_ms`, none for a life that lasts its curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurveData {
    pub first: [Num; 2],
    pub second: [Num; 2],
    pub follow: Num,
    pub life_ms: Option<u32>,
}

/// What a projectile hits. In data: `none`, a filter of the units it hits, or a list of filters,
/// any one of which selects a unit it hits (04-capabilities/actions.md, A projectile hits by any
/// of its filters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectileHits {
    /// No unit: it flies to its end, as Snow Owl flies to its point.
    Nothing,
    /// The units any of its filters selects, of which it holds one at least.
    Units(Vec<FilterData>),
}

/// Data is untrusted, so a speed that is not positive, a width or a range that is negative, a
/// width whose half, the radius of the circle it flies as, is past `MAX_BOUND`, or a curve with
/// what only a line or a homing flight takes, fails to read.
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
            curve: Option<CurveData>,
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
        // A curve ends at its target and at its first hit, and falls by no gravity.
        let of_a_line = fields.homing
            || fields.gravity.is_some()
            || fields.range.is_some()
            || fields.stop_on_hit
            || fields.once_per_cast;
        if fields.curve.is_some() && of_a_line {
            return Err(D::Error::custom(
                "a curve takes no `homing`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`",
            ));
        }
        Ok(Self {
            speed: fields.speed,
            width,
            range: fields.range,
            homing: fields.homing,
            stop_on_hit: fields.stop_on_hit,
            once_per_cast: fields.once_per_cast,
            hits: fields.hits.unwrap_or_else(ProjectileHits::enemies),
            gravity: fields.gravity,
            curve: fields.curve,
        })
    }
}

impl ProjectileData {
    /// How it flies.
    pub const fn flight(&self) -> ProjectileFlight {
        match (self.homing, self.curve) {
            (true, _) => ProjectileFlight::Homing,
            (false, Some(_)) => ProjectileFlight::Curve,
            (false, None) => ProjectileFlight::Line,
        }
    }
}

/// Data is untrusted, so a fraction outside 0 to 1, a height past `MAX_BOUND` either way, a
/// negative `follow` or a life of 0 fails to read.
impl<'de> Deserialize<'de> for CurveData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            first: [Num; 2],
            second: [Num; 2],
            #[serde(default)]
            follow: Num,
            life_ms: Option<u32>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let holds = |[fraction, height]: [Num; 2]| {
            (Num::ZERO..=Num::ONE).contains(&fraction)
                && height.to_bits().unsigned_abs() <= MAX_BOUND.to_bits().unsigned_abs()
        };
        if !holds(fields.first) || !holds(fields.second) {
            return Err(D::Error::custom(
                "a curve's point is a fraction from 0 to 1 and a height up to 2,048 m either way",
            ));
        }
        if fields.follow < Num::ZERO {
            return Err(D::Error::custom("a curve's `follow` is not negative"));
        }
        if fields.life_ms == Some(0) {
            return Err(D::Error::custom("a curve's `life_ms` is positive"));
        }
        Ok(Self {
            first: fields.first,
            second: fields.second,
            follow: fields.follow,
            life_ms: fields.life_ms,
        })
    }
}

impl ProjectileHits {
    /// The units of `enemies`, which a projectile hits when its data names none.
    pub fn enemies() -> Self {
        Self::Units(vec![FilterData {
            relations: RelationSet::Enemies,
            tags: Vec::new(),
        }])
    }
}

/// Data is untrusted, so a list of no filter, which `none` says, fails to read.
impl<'de> Deserialize<'de> for ProjectileHits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Fields {
            One(String),
            Any(Vec<String>),
        }
        let filter = |text: &str| {
            FilterData::parse(text).ok_or_else(|| D::Error::custom(format!("filter {text:?}")))
        };
        match Fields::deserialize(deserializer)? {
            Fields::One(text) if text == "none" => Ok(Self::Nothing),
            Fields::One(text) => Ok(Self::Units(vec![filter(&text)?])),
            Fields::Any(texts) if texts.is_empty() => Err(D::Error::custom(
                "a list of hits holds a filter at least; `none` hits no unit",
            )),
            Fields::Any(texts) => texts
                .iter()
                .map(|text| filter(text))
                .collect::<Result<_, _>>()
                .map(Self::Units),
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
        pub fn flying(speed: Num) -> Self {
            Self {
                speed,
                width: Num::ZERO,
                range: None,
                homing: false,
                stop_on_hit: false,
                once_per_cast: false,
                hits: ProjectileHits::enemies(),
                gravity: None,
                curve: None,
            }
        }
    }
}
