use std::num::NonZeroU8;

use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_sim::Capability;
use campfire_values::UnitType;

use crate::book_builders;
use crate::door_times::DoorTimes;
use crate::frame_point::FramePoint;
use crate::production_data::{PlaceData, ProductionData};

/// A producer type's `production` with its names resolved: the most entries its train queue
/// holds, its exit and default rally points, whether the gatherers that leave its exit start to
/// gather, the unit types that leave its exit once, as its site completes, and the places its
/// trains are made at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerSpec {
    pub queue: NonZeroU8,
    pub exit: Option<FramePoint>,
    pub rally: Option<FramePoint>,
    pub gathers: bool,
    pub free: Box<[UnitType]>,
    pub places: Box<[PlaceData]>,
    pub doors: Option<DoorTimes>,
}

impl PerId<UnitType> for ProducerSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl ProducerSpec {
    /// The production of each unit type that stands and produces; an error for a producer that
    /// starts its gatherers in a match without navigation, which walks them, and for a free unit's
    /// type as `leaving` refuses one.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ProductionData>() else {
                continue;
            };
            if data.gathers {
                let at = || BookPlace::UnitType(unit.place());
                cx.require(Capability::Navigation, unit.package, at)?;
            }
            let free = data
                .free
                .iter()
                .map(|name| book_builders::leaving(cx, unit, name));
            let spec = Self {
                queue: data.queue,
                exit: data.exit,
                rally: data.rally,
                gathers: data.gathers,
                free: free.collect::<Result<_, _>>()?,
                places: data.places.iter().copied().collect(),
                doors: data.doors.map(|doors| DoorTimes::of(doors, cx.rate())),
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}
