use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId, Section, SectionTable};
use campfire_math::{Num, Vec3};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::UnitType;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's `flight` section: the height above the ground its units fly at, the meters a
/// second they climb or sink toward it, and the height above the ground past which they are
/// airborne, each in meters (helicopters.md, A flier holds a height; helicopters.md, Airborne is
/// a tag of height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FlightData {
    pub(crate) height: Num,
    pub(crate) climb: Num,
    pub(crate) airborne: Num,
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
        })
    }

    /// The height above the ground its units fly at.
    pub const fn height(&self) -> Num {
        self.height
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
        }
        let Fields {
            height,
            climb,
            airborne,
        } = Fields::deserialize(deserializer)?;
        Self::new(height, climb, airborne).ok_or_else(|| {
            D::Error::custom(
                "a flight's height and airborne height lie from 0 to the world's bound, its climb \
                 above 0 to it",
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
            })
        );
        assert!(flight(0, 1, 0).is_some());
        for [height, climb, airborne] in [[-1, 1, 0], [10, 0, 0], [10, -1, 0], [10, 1, -1]] {
            assert_eq!(flight(height, climb, airborne), None);
        }
        let past = Vec3::BOUND + Num::ONE;
        assert_eq!(FlightData::new(past, Num::ONE, Num::ZERO), None);
    }
}
