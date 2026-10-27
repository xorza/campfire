use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId, Section, SectionTable,
};
use campfire_production::{FramePoint, ProductionData};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::error::AirfieldError;

/// A unit type's `airfield` section: for each of its production's places, in their order, the
/// point its unit waits at for its runway and the runway it takes; and its runways, each from its
/// start to its end, every point in its own frame (jets.md, A jet takes off and lands on its
/// runway).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirfieldData {
    pub places: Vec<AirfieldPlace>,
    pub runways: Vec<RunwayData>,
}

/// A place's point it waits at for its runway, and that runway's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirfieldPlace {
    pub prep: FramePoint,
    pub runway: u8,
}

/// A runway, from the point a takeoff rolls from to the point it lifts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunwayData {
    pub start: FramePoint,
    pub end: FramePoint,
}

impl AirfieldData {
    /// The airfield of each unit type that stands with an `airfield` section; an error for one
    /// whose places are not its production's, or that names a runway it does not have.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<Self>() else {
                continue;
            };
            let places = unit
                .data
                .section::<ProductionData>()
                .map_or(0, |production| production.places.len());
            let at = || BookPlace::UnitType(unit.place());
            if data.places.len() != places {
                return Err(BookError::rule(unit.package, at(), AirfieldError::Places));
            }
            if data
                .places
                .iter()
                .any(|place| usize::from(place.runway) >= data.runways.len())
            {
                return Err(BookError::rule(unit.package, at(), AirfieldError::Runway));
            }
            book.set(unit.unit_type, data.clone());
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for AirfieldData {
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for AirfieldData {
    const TABLE: DataTable = DataTable::new("A unit type's `airfield`");
}

impl Section for AirfieldData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "airfield";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
