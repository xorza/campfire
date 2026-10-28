use std::num::NonZeroU32;

use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_values::UnitType;

use crate::rides_data::RidesData;

/// A rider type's size, package data and not state: the room each of its units takes of a
/// container that counts its riders' sizes; a type of none rides nothing
/// (transports.md, A container says who rides).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiderSize(NonZeroU32);

impl RiderSize {
    pub const fn new(size: NonZeroU32) -> Self {
        Self(size)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl PerId<UnitType> for RiderSize {
    const SCRIPTS_ONLY: bool = false;
}

impl RiderSize {
    /// The size of each rider type that stands.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            if let Some(rides) = unit.data.section::<RidesData>() {
                book.set(unit.unit_type, Self::new(rides.size));
            }
        }
        Ok(book.build())
    }
}
