use std::num::NonZeroI64;

use campfire_common::Ticks;
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId, Section, SectionTable,
};
use campfire_math::{Num, Rounding};
use campfire_navigation::FlightData;
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_sim::MatchRate;
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::error::AirfieldError;

/// A unit type's `sortie` section, a unit an airfield parks: the milliseconds it waits at its
/// runway's start before it rolls, its speed in meters a second on the ground as it taxis, and
/// the milliseconds it stands idle in the air before it goes home, none for never (jets.md, A
/// jet takes off and lands on its runway; jets.md, A jet goes home).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortieData {
    pub pause_ms: u32,
    pub taxi_speed: Num,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_ms: Option<u32>,
}

/// A sortie at the match's rate: its pause and its idle time in ticks, rounded up, and its taxi's
/// step a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortieSpec {
    pub pause: Ticks,
    pub taxi_step: Num,
    pub idle: Option<Ticks>,
}

impl SortieSpec {
    /// The sortie of each unit type that stands with a `sortie` section, at the match's rate; an
    /// error for a type of no `flight`, which nothing flies.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let rate = cx.rate();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(&data) = unit.data.section::<SortieData>() else {
                continue;
            };
            if unit.data.section::<FlightData>().is_none() {
                let at = BookPlace::UnitType(unit.place());
                return Err(BookError::rule(
                    unit.package,
                    at,
                    AirfieldError::SortieFlight,
                ));
            }
            book.set(unit.unit_type, Self::of(data, rate));
        }
        Ok(book.build())
    }

    /// `data` at `rate`.
    fn of(data: SortieData, rate: MatchRate) -> Self {
        let hz = NonZeroI64::from(rate.hz()).get();
        Self {
            pause: rate
                .ticks(u64::from(data.pause_ms))
                .expect("a pause of a u32 of milliseconds fits the ticks"),
            idle: data.idle_ms.map(|ms| {
                rate.ticks(u64::from(ms))
                    .expect("an idle time of a u32 of milliseconds fits the ticks")
            }),
            taxi_step: data
                .taxi_speed
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("a speed over a rate fits"),
        }
    }
}

impl PerId<UnitType> for SortieSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for SortieData {
    const TABLE: DataTable = DataTable::new("A unit type's `sortie`");
}

impl Section for SortieData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "sortie";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
