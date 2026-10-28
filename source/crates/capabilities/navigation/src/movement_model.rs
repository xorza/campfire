use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_values::UnitType;

use crate::locomotor_data::LocomotorData;
use crate::zero_hour::zero_hour_mover::ZeroHourMover;

/// How a unit type's walkers move along their routes (04-capabilities/navigation.md, A type moves
/// by its movement model): the engine's step, which turns toward its leg and steps along it, or
/// Zero Hour's locomotor, which drives by a velocity it keeps (04-capabilities/navigation.md, A
/// mover keeps a velocity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MovementModel {
    Step,
    ZeroHour(ZeroHourMover),
}

impl MovementModel {
    /// The model of each unit type that stands with a `locomotor`, at the match's rate; a type of
    /// none steps.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder may fail, as `Declarations::book` takes it; this one reads no other book"
    )]
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let rate = cx.rate();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let model = unit
                .data
                .section::<LocomotorData>()
                .map(|locomotor| match locomotor {
                    LocomotorData::ZeroHour(locomotor) => {
                        Self::ZeroHour(ZeroHourMover::of(locomotor, rate))
                    }
                });
            book.set_some(unit.unit_type, model);
        }
        Ok(book.build())
    }

    /// The model of `unit_type` in `book`: the step for a type the book holds none of.
    pub(crate) fn of(book: &ById<UnitType, Self>, unit_type: UnitType) -> Self {
        book.get(unit_type).copied().unwrap_or(Self::Step)
    }
}

impl PerId<UnitType> for MovementModel {
    const SCRIPTS_ONLY: bool = false;
}
