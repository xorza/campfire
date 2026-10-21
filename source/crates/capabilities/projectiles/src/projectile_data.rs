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
/// a unit, along a line, on a `curve` or steered as `steer` says, at most `speed`, and hit what
/// `hits` says, the units of `enemies` by default.
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
    pub steer: Option<SteerData>,
}

/// A projectile's `steer` (12-missiles.md, A steered flight): it leaves at `launch_speed` m/s,
/// gains `acceleration` m/s each second up to its projectile's speed, flies straight for
/// `straight` meters, then turns toward its target by at most `turn_rate` degrees a second; it
/// flies straight at its target within `lock` meters on the ground, or at its point within
/// `point_lock`, `lock` when it has none; it holds `cruise`'s height
/// over its target until within its dive; it ends as `fuel_ms` runs out; and, unless it
/// `tracks` its target, it flies to the point where its target stood as it launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteerData {
    pub(crate) launch_speed: Option<Num>,
    pub(crate) acceleration: Num,
    pub(crate) turn_rate: Num,
    pub(crate) straight: Num,
    pub(crate) lock: Num,
    pub(crate) point_lock: Option<Num>,
    pub(crate) fuel_ms: u32,
    pub(crate) cruise: Option<CruiseData>,
    pub(crate) tracks: bool,
}

/// A steered projectile's `cruise` (12-missiles.md, A cruise height): the height in meters over
/// the ground under its target it holds, until it comes within `dive` meters of its target on the
/// ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CruiseData {
    pub(crate) height: Num,
    pub(crate) dive: Num,
}

/// A steer as `SteerData::builder` gathers it, which `build` checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteerDataBuilder(SteerData);

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
            steer: None,
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

    #[must_use]
    pub const fn steer(mut self, steer: SteerData) -> Self {
        self.0.steer = Some(steer);
        self
    }

    /// The projectile; an error for a speed that is not positive, a width or a range that is
    /// negative, a width whose half, the radius of the circle it flies as, is past `MAX_BOUND`,
    /// a curve with what only a line or a homing flight takes, or a steer beside what another
    /// flight takes or leaving faster than the projectile's speed.
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
        if let Some(steer) = data.steer {
            if of_a_line || data.curve.is_some() {
                return Err(ProjectileDataError::SteerOfLine);
            }
            if steer.launch_speed.is_some_and(|launch| launch > data.speed) {
                return Err(ProjectileDataError::SteerLaunch);
            }
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
            steer: Option<SteerData>,
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
        if let Some(steer) = fields.steer {
            builder = builder.steer(steer);
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
        if let Some(steer) = &self.steer {
            map.serialize_entry("steer", steer)?;
        }
        map.end()
    }
}

impl ProjectileData {
    /// How it flies.
    pub const fn flight(&self) -> ProjectileFlight {
        match (self.homing, self.curve, self.steer) {
            (true, ..) => ProjectileFlight::Homing,
            (false, Some(_), _) => ProjectileFlight::Curve,
            (false, None, Some(_)) => ProjectileFlight::Steered,
            (false, None, None) => ProjectileFlight::Line,
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

impl SteerData {
    /// A steer that turns by at most `turn_rate` degrees a second and flies for `fuel_ms`, leaving
    /// at its projectile's speed, straight at once, with no lock and no cruise, and tracking its
    /// target, until a setter gives it more.
    pub const fn builder(turn_rate: Num, fuel_ms: u32) -> SteerDataBuilder {
        SteerDataBuilder(Self {
            launch_speed: None,
            acceleration: Num::ZERO,
            turn_rate,
            straight: Num::ZERO,
            lock: Num::ZERO,
            point_lock: None,
            fuel_ms,
            cruise: None,
            tracks: true,
        })
    }
}

impl SteerDataBuilder {
    #[must_use]
    pub const fn launch_speed(mut self, launch_speed: Num) -> Self {
        self.0.launch_speed = Some(launch_speed);
        self
    }

    #[must_use]
    pub const fn acceleration(mut self, acceleration: Num) -> Self {
        self.0.acceleration = acceleration;
        self
    }

    #[must_use]
    pub const fn straight(mut self, straight: Num) -> Self {
        self.0.straight = straight;
        self
    }

    #[must_use]
    pub const fn lock(mut self, lock: Num) -> Self {
        self.0.lock = lock;
        self
    }

    #[must_use]
    pub const fn point_lock(mut self, point_lock: Num) -> Self {
        self.0.point_lock = Some(point_lock);
        self
    }

    #[must_use]
    pub const fn cruise(mut self, cruise: CruiseData) -> Self {
        self.0.cruise = Some(cruise);
        self
    }

    #[must_use]
    pub const fn tracks(mut self, tracks: bool) -> Self {
        self.0.tracks = tracks;
        self
    }

    /// The steer; an error for a fuel of 0, a turn rate or an acceleration below 0, or a launch
    /// speed, a straight run, a lock, a point's lock or a cruise's height or dive below 0 or past
    /// `MAX_BOUND`.
    pub fn build(self) -> Result<SteerData, ProjectileDataError> {
        let steer = self.0;
        if steer.fuel_ms == 0 {
            return Err(ProjectileDataError::SteerFuel);
        }
        let holds = |value: Num| (Num::ZERO..=MAX_BOUND).contains(&value);
        let numbers = [
            steer.launch_speed.unwrap_or(Num::ZERO),
            steer.straight,
            steer.lock,
            steer.point_lock.unwrap_or(Num::ZERO),
        ];
        let cruise = steer
            .cruise
            .map_or([Num::ZERO; 2], |cruise| [cruise.height, cruise.dive]);
        let rates = [steer.turn_rate, steer.acceleration];
        if rates.into_iter().any(|rate| rate < Num::ZERO)
            || !numbers.into_iter().chain(cruise).all(holds)
        {
            return Err(ProjectileDataError::SteerNumber);
        }
        Ok(steer)
    }
}

impl CruiseData {
    /// A cruise `height` meters over its target's ground until within `dive` meters of it.
    pub const fn new(height: Num, dive: Num) -> Self {
        Self { height, dive }
    }
}

/// Data is untrusted, so a steer `SteerData::builder` refuses fails to read.
impl<'de> Deserialize<'de> for SteerData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            launch_speed: Option<Num>,
            #[serde(default)]
            acceleration: Num,
            turn_rate: Num,
            #[serde(default)]
            straight: Num,
            #[serde(default)]
            lock: Num,
            point_lock: Option<Num>,
            fuel_ms: u32,
            cruise: Option<CruiseData>,
            tracks: Option<bool>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let mut builder = Self::builder(fields.turn_rate, fields.fuel_ms)
            .acceleration(fields.acceleration)
            .straight(fields.straight)
            .lock(fields.lock)
            .tracks(fields.tracks.unwrap_or(true));
        if let Some(launch) = fields.launch_speed {
            builder = builder.launch_speed(launch);
        }
        if let Some(point_lock) = fields.point_lock {
            builder = builder.point_lock(point_lock);
        }
        if let Some(cruise) = fields.cruise {
            builder = builder.cruise(cruise);
        }
        builder.build().map_err(D::Error::custom)
    }
}

/// As data writes it, what `Deserialize` reads: each field that is not its default.
impl Serialize for SteerData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(launch) = self.launch_speed {
            map.serialize_entry("launch_speed", &launch)?;
        }
        if self.acceleration != Num::ZERO {
            map.serialize_entry("acceleration", &self.acceleration)?;
        }
        map.serialize_entry("turn_rate", &self.turn_rate)?;
        if self.straight != Num::ZERO {
            map.serialize_entry("straight", &self.straight)?;
        }
        if self.lock != Num::ZERO {
            map.serialize_entry("lock", &self.lock)?;
        }
        if let Some(point_lock) = self.point_lock {
            map.serialize_entry("point_lock", &point_lock)?;
        }
        map.serialize_entry("fuel_ms", &self.fuel_ms)?;
        if let Some(cruise) = &self.cruise {
            map.serialize_entry("cruise", cruise)?;
        }
        if !self.tracks {
            map.serialize_entry("tracks", &false)?;
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

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_projectile_reads_only_with_a_positive_speed_a_width_of_0_to_4096_m_and_no_negative_range()
    {
        let read = |text: &str| Toml::parse::<ProjectileData>(text);
        let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
        let plain = ProjectileData::flying(Num::HALF);
        assert_eq!(read("speed = \"0.5\"").unwrap(), plain);
        assert_eq!(
            read("speed = 20\nwidth = 0\nrange = 0").unwrap(),
            ProjectileData {
                speed: Num::int(20),
                range: Some(Num::ZERO),
                ..plain.clone()
            }
        );
        // A width of 4,096 m flies as a circle of 2,048 m, the widest body; a bit more does not read.
        let widest = read("speed = 20\nwidth = 4096").unwrap();
        assert_eq!(widest.width, Num::int(4096));
        // `hits` is `none`, a filter or a list of filters, `enemies` when it is absent; a list of
        // none, and a list of a filter that is none, do not read.
        let hits = |text: &str| read(&format!("speed = 1\nhits = {text}")).map(|data| data.hits);
        let filter = |text: &str| FilterData::parse(text).unwrap();
        assert_eq!(hits("\"none\"").unwrap(), ProjectileHits::Nothing);
        assert_eq!(
            hits("\"allies:avatar\"").unwrap(),
            ProjectileHits::Units(vec![filter("allies:avatar")])
        );
        assert_eq!(
            hits("[\"enemies:structure\", \"all:wall\"]").unwrap(),
            ProjectileHits::Units(vec![filter("enemies:structure"), filter("all:wall")])
        );
        assert_eq!(plain.hits, ProjectileHits::Units(vec![filter("enemies")]));
        for text in ["\"nobody\"", "[\"all\", \"nobody\"]"] {
            let message = hits(text).unwrap_err().message().to_owned();
            assert!(message.starts_with("filter \"nobody\""), "{message}");
        }
        assert_eq!(
            hits("[]").unwrap_err().message(),
            "a list of hits holds a filter at least; `none` hits no unit"
        );
        for (text, message) in [
            ("speed = 0", "a projectile's speed is positive"),
            ("speed = \"-0.5\"", "a projectile's speed is positive"),
            (
                "speed = 9223372036854775807",
                "9223372036854775807 is past what a number holds",
            ),
            (
                "speed = 20\nwidth = -1",
                "a projectile's width is not negative",
            ),
            (
                "speed = 20\nwidth = \"-0.001\"",
                "a projectile's width is not negative",
            ),
            (
                "speed = 20\nwidth = \"4096.00000006\"",
                "a projectile's width is at most 4,096 m",
            ),
            (
                "speed = 20\nrange = -1",
                "a projectile's range is not negative",
            ),
            // A steer takes nothing of another flight, leaves no faster than its speed, flies for
            // some time, turns and speeds up by rates not negative, and holds its other numbers
            // within 2,048.
            (
                "speed = 20\nhoming = true\nsteer = { turn_rate = 90, fuel_ms = 1000 }",
                "a steer takes no `homing`",
            ),
            (
                "speed = 20\nsteer = { launch_speed = 21, turn_rate = 90, fuel_ms = 1000 }",
                "a steer leaves no faster than its projectile's speed",
            ),
            (
                "speed = 20\nsteer = { turn_rate = 90, fuel_ms = 0 }",
                "a steer's `fuel_ms` is positive",
            ),
            (
                "speed = 20\nsteer = { turn_rate = -1, fuel_ms = 1000 }",
                "a steer's turn rate and acceleration are not negative",
            ),
            (
                "speed = 20\nsteer = { turn_rate = 90, fuel_ms = 1000, lock = 2049 }",
                "a steer's turn rate and acceleration are not negative",
            ),
            (
                "speed = 20\nsteer = { turn_rate = 90, acceleration = -1, fuel_ms = 1000 }",
                "a steer's turn rate and acceleration are not negative",
            ),
            (
                "speed = 20\nsteer = { turn_rate = 90, fuel_ms = 1000, point_lock = -1 }",
                "a steer's turn rate and acceleration are not negative",
            ),
        ] {
            assert!(refusal(text).starts_with(message), "{text}");
        }
        // A projectile writes each field but its defaults, and reads back what it wrote: a line of
        // its speed alone, one of every field a line takes, a curve that hits nothing, and a steer
        // of every field and of the fewest, which tracks its target and leaves at its speed.
        let steered =
            read("speed = 20\nsteer = { turn_rate = 9000, acceleration = 9160, fuel_ms = 1 }")
                .unwrap();
        let steer = SteerData::builder(Num::int(9000), 1)
            .acceleration(Num::int(9160))
            .build()
            .unwrap();
        assert_eq!(
            (steered.steer, steered.flight()),
            (Some(steer), ProjectileFlight::Steered)
        );
        for text in [
            "speed = 20\n\n[steer]\nlaunch_speed = 15\nacceleration = 675\nturn_rate = 100\nstraight = 3\nlock = 75\npoint_lock = \"37.5\"\nfuel_ms = 1800\ntracks = false\n\n[steer.cruise]\nheight = 120\ndive = 100\n",
            "speed = 20\n\n[steer]\nturn_rate = 100\nfuel_ms = 1800\n",
            "speed = \"0.5\"\n",
            "speed = 20\nwidth = 2\nrange = 9\nhoming = true\nstop_on_hit = true\nonce_per_cast = true\nhits = [\"enemies:structure\", \"all:wall\"]\n",
            "speed = 20\nhits = \"none\"\n\n[curve]\nfirst = [\"0.5\", 10]\nsecond = [1, -2]\nfollow = 3\nlife_ms = 900\n",
        ] {
            let data = read(text).unwrap();
            let written = Toml::write(&data);
            assert_eq!(read(&written).unwrap(), data, "{written}");
            assert_eq!(Toml::write(&read(&written).unwrap()), written);
        }
    }
}
