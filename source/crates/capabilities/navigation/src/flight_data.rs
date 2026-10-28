use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId, Section, SectionTable};
use campfire_math::{Num, Vec3};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::UnitType;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's `flight` section: the height above the ground its units fly at, the meters a
/// second they climb or sink toward it, and the height above the ground past which they are
/// airborne, each in meters, and its `wings` for a type that never stands in the air
/// (04-capabilities/navigation.md, A flier holds a height; 04-capabilities/stats.md, Airborne is a
/// tag of height; 04-capabilities/navigation.md, A jet flies on wings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FlightData {
    pub(crate) height: Num,
    pub(crate) climb: Num,
    pub(crate) airborne: Num,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wings: Option<WingsData>,
}

/// A flight's `wings`: the meters a second its units fly at, at least, with no goal, and the
/// radius in meters of the circle they fly round the place they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WingsData {
    pub(crate) least_speed: Num,
    pub(crate) circling: Num,
}

impl FlightData {
    /// A flight at `height` that climbs `climb` a second and is airborne past `airborne`; `None`
    /// for a height or an airborne height below 0 or past the world's bound, or a climb not
    /// above 0 or past it.
    pub fn new(height: Num, climb: Num, airborne: Num) -> Option<Self> {
        let height_holds = |value: Num| (Num::ZERO..=Vec3::BOUND).contains(&value);
        let holds = height_holds(height)
            && height_holds(airborne)
            && climb > Num::ZERO
            && climb <= Vec3::BOUND;
        holds.then_some(Self {
            height,
            climb,
            airborne,
            wings: None,
        })
    }

    /// It of `wings`, its units never standing in the air.
    #[must_use]
    pub const fn winged(self, wings: WingsData) -> Self {
        Self {
            wings: Some(wings),
            ..self
        }
    }

    /// The height above the ground its units fly at.
    pub const fn height(&self) -> Num {
        self.height
    }

    /// The meters a second its units climb or sink toward their height.
    pub const fn climb(&self) -> Num {
        self.climb
    }

    /// Its wings, none for a flight that hovers.
    pub const fn wings(&self) -> Option<WingsData> {
        self.wings
    }

    /// The flight of each unit type that stands with a `flight` section.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder may fail, as `Declarations::book` takes it; this one reads no other book"
    )]
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            book.set_some(unit.unit_type, unit.data.section::<Self>().copied());
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for FlightData {
    const SCRIPTS_ONLY: bool = false;
}

/// Data is untrusted, so a flight `new` refuses fails to read.
impl<'de> Deserialize<'de> for FlightData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            height: Num,
            climb: Num,
            airborne: Num,
            #[serde(default)]
            wings: Option<WingsData>,
        }
        let Fields {
            height,
            climb,
            airborne,
            wings,
        } = Fields::deserialize(deserializer)?;
        let flight = Self::new(height, climb, airborne).ok_or_else(|| {
            D::Error::custom(
                "a flight's height and airborne height lie from 0 to the world's bound, its climb \
                 above 0 to it",
            )
        })?;
        Ok(match wings {
            Some(wings) => flight.winged(wings),
            None => flight,
        })
    }
}

impl WingsData {
    /// Wings of `least_speed` and `circling`; `None` for either not above 0 or past the world's
    /// bound.
    pub fn new(least_speed: Num, circling: Num) -> Option<Self> {
        let holds = |value: Num| value > Num::ZERO && value <= Vec3::BOUND;
        (holds(least_speed) && holds(circling)).then_some(Self {
            least_speed,
            circling,
        })
    }

    /// The meters a second its units fly at, at least.
    pub const fn least_speed(&self) -> Num {
        self.least_speed
    }
}

/// Data is untrusted, so wings `new` refuses fail to read.
impl<'de> Deserialize<'de> for WingsData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            least_speed: Num,
            circling: Num,
        }
        let Fields {
            least_speed,
            circling,
        } = Fields::deserialize(deserializer)?;
        Self::new(least_speed, circling).ok_or_else(|| {
            D::Error::custom(
                "wings' least speed and circling radius lie above 0 to the world's bound",
            )
        })
    }
}

impl DataSection for FlightData {
    const TABLE: DataTable = DataTable::new("A unit type's `flight`");
}

impl Section for FlightData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "flight";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_reads_its_numbers_and_refuses_those_out_of_range() {
        let flight = |height, climb, airborne| {
            FlightData::new(Num::int(height), Num::int(climb), Num::int(airborne))
        };
        assert_eq!(
            flight(100, 55, 30),
            Some(FlightData {
                height: Num::int(100),
                climb: Num::int(55),
                airborne: Num::int(30),
                wings: None,
            })
        );
        assert!(flight(0, 1, 0).is_some());
        for [height, climb, airborne] in [[-1, 1, 0], [10, 0, 0], [10, -1, 0], [10, 1, -1]] {
            assert_eq!(flight(height, climb, airborne), None);
        }
        let past = Vec3::BOUND + Num::ONE;
        assert_eq!(FlightData::new(past, Num::ONE, Num::ZERO), None);
        // Wings fly and circle at more than 0, within the bound.
        let wings = |least, circling| WingsData::new(Num::int(least), Num::int(circling));
        assert!(wings(60, 100).is_some());
        for [least, circling] in [[0, 100], [60, 0], [-1, 100]] {
            assert_eq!(wings(least, circling), None);
        }
        assert_eq!(WingsData::new(Num::ONE, past), None);
    }
}
