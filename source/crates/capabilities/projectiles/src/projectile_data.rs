use campfire_actions::ProjectileFlight;
use campfire_declarations::{Section, SectionTable};
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::{FilterData, RelationSet};
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ProjectileDataError;

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

/// A projectile's data as `ProjectileData::builder` gathers it, which `build` checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileDataBuilder(ProjectileData);

impl ProjectileData {
    /// A projectile of `speed` m/s, no wider than a line, of its action's range, along a line,
    /// through the units of `enemies`, until a setter gives it more.
    pub fn builder(speed: Num) -> ProjectileDataBuilder {
        ProjectileDataBuilder(Self {
            speed,
            width: Num::ZERO,
            range: None,
            homing: false,
            stop_on_hit: false,
            once_per_cast: false,
            hits: ProjectileHits::enemies(),
            gravity: None,
            curve: None,
        })
    }
}

impl ProjectileDataBuilder {
    #[must_use]
    pub const fn width(mut self, width: Num) -> Self {
        self.0.width = width;
        self
    }

    #[must_use]
    pub const fn range(mut self, range: Num) -> Self {
        self.0.range = Some(range);
        self
    }

    #[must_use]
    pub const fn homing(mut self, homing: bool) -> Self {
        self.0.homing = homing;
        self
    }

    #[must_use]
    pub const fn stop_on_hit(mut self, stop_on_hit: bool) -> Self {
        self.0.stop_on_hit = stop_on_hit;
        self
    }

    #[must_use]
    pub const fn once_per_cast(mut self, once_per_cast: bool) -> Self {
        self.0.once_per_cast = once_per_cast;
        self
    }

    #[must_use]
    pub fn hits(mut self, hits: ProjectileHits) -> Self {
        self.0.hits = hits;
        self
    }

    #[must_use]
    pub const fn gravity(mut self, gravity: Num) -> Self {
        self.0.gravity = Some(gravity);
        self
    }

    #[must_use]
    pub const fn curve(mut self, curve: CurveData) -> Self {
        self.0.curve = Some(curve);
        self
    }

    /// The projectile; an error for a speed that is not positive, a width or a range that is
    /// negative, a width whose half, the radius of the circle it flies as, is past `MAX_BOUND`,
    /// or a curve with what only a line or a homing flight takes.
    pub fn build(self) -> Result<ProjectileData, ProjectileDataError> {
        let data = self.0;
        if data.speed <= Num::ZERO {
            return Err(ProjectileDataError::Speed);
        }
        if data.width < Num::ZERO {
            return Err(ProjectileDataError::NegativeWidth);
        }
        if data.width.to_bits() > 2 * MAX_BOUND.to_bits() {
            return Err(ProjectileDataError::WideWidth);
        }
        if data.range.is_some_and(|range| range < Num::ZERO) {
            return Err(ProjectileDataError::Range);
        }
        let of_a_line = data.homing
            || data.gravity.is_some()
            || data.range.is_some()
            || data.stop_on_hit
            || data.once_per_cast;
        if data.curve.is_some() && of_a_line {
            return Err(ProjectileDataError::CurveOfLine);
        }
        Ok(data)
    }
}

/// Data is untrusted, so a projectile `ProjectileData::builder` refuses fails to read.
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
        let mut builder = Self::builder(fields.speed)
            .width(fields.width.unwrap_or(Num::ZERO))
            .homing(fields.homing)
            .stop_on_hit(fields.stop_on_hit)
            .once_per_cast(fields.once_per_cast);
        if let Some(range) = fields.range {
            builder = builder.range(range);
        }
        if let Some(hits) = fields.hits {
            builder = builder.hits(hits);
        }
        if let Some(gravity) = fields.gravity {
            builder = builder.gravity(gravity);
        }
        if let Some(curve) = fields.curve {
            builder = builder.curve(curve);
        }
        builder.build().map_err(D::Error::custom)
    }
}

/// As data writes it, what `Deserialize` reads: each field that is not its default.
impl Serialize for ProjectileData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("speed", &self.speed)?;
        if self.width != Num::ZERO {
            map.serialize_entry("width", &self.width)?;
        }
        if let Some(range) = self.range {
            map.serialize_entry("range", &range)?;
        }
        if self.homing {
            map.serialize_entry("homing", &true)?;
        }
        if self.stop_on_hit {
            map.serialize_entry("stop_on_hit", &true)?;
        }
        if self.once_per_cast {
            map.serialize_entry("once_per_cast", &true)?;
        }
        if self.hits != ProjectileHits::enemies() {
            map.serialize_entry("hits", &self.hits)?;
        }
        if let Some(gravity) = self.gravity {
            map.serialize_entry("gravity", &gravity)?;
        }
        if let Some(curve) = &self.curve {
            map.serialize_entry("curve", curve)?;
        }
        map.end()
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

impl CurveData {
    /// A curve through `first` and `second`, following its target by `follow` meters a second,
    /// for `life_ms`, or its curve's length with none; an error for a fraction outside 0 to 1, a
    /// height past `MAX_BOUND` either way, a negative `follow` or a life of 0.
    pub fn new(
        first: [Num; 2],
        second: [Num; 2],
        follow: Num,
        life_ms: Option<u32>,
    ) -> Result<Self, ProjectileDataError> {
        let holds = |[fraction, height]: [Num; 2]| {
            (Num::ZERO..=Num::ONE).contains(&fraction)
                && height.to_bits().unsigned_abs() <= MAX_BOUND.to_bits().unsigned_abs()
        };
        if !holds(first) || !holds(second) {
            return Err(ProjectileDataError::CurvePoint);
        }
        if follow < Num::ZERO {
            return Err(ProjectileDataError::CurveFollow);
        }
        if life_ms == Some(0) {
            return Err(ProjectileDataError::CurveLife);
        }
        Ok(Self {
            first,
            second,
            follow,
            life_ms,
        })
    }
}

/// Data is untrusted, so a curve `CurveData::new` refuses fails to read.
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
        Self::new(fields.first, fields.second, fields.follow, fields.life_ms)
            .map_err(D::Error::custom)
    }
}

/// As data writes it, what `Deserialize` reads.
impl Serialize for CurveData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("first", &self.first)?;
        map.serialize_entry("second", &self.second)?;
        if self.follow != Num::ZERO {
            map.serialize_entry("follow", &self.follow)?;
        }
        if let Some(life_ms) = self.life_ms {
            map.serialize_entry("life_ms", &life_ms)?;
        }
        map.end()
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

/// As data writes it: `none`, or the list of its filters.
impl Serialize for ProjectileHits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Nothing => serializer.serialize_str("none"),
            Self::Units(filters) => filters.serialize(serializer),
        }
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
    const TABLE: DataTable = DataTable::new("A unit type's `projectile`");
}

impl Section for ProjectileData {
    const TABLE: SectionTable = SectionTable::DeliveryType;
    const KEY: &'static str = "projectile";
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::projectile_data::ProjectileData;

    impl ProjectileData {
        /// A projectile of `speed` m/s and nothing more, as a section that names its speed alone
        /// reads: no width, its action's range, along a line, through what it hits.
        pub fn flying(speed: Num) -> Self {
            Self::builder(speed).build().unwrap()
        }
    }
}
