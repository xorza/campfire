use campfire_declarations::{BookCx, BookError, ById, Fact};

use crate::accel_step::AccelStep;
use crate::move_step::MoveStep;
use crate::turn_step::TurnStep;
use campfire_values::UnitType;

/// How far each unit type's units walk, turn and speed up a tick at level 1, which the module of
/// the stats they are of makes.
#[derive(Debug, Default)]
pub struct UnitSteps {
    pub steps: ById<UnitType, MoveStep>,
    pub turns: ById<UnitType, TurnStep>,
    pub accelerations: ById<UnitType, AccelStep>,
}

impl Fact for UnitSteps {}

impl UnitSteps {
    /// Each unit type's move step, as the module of the stats they are of makes them; none in a
    /// match without that module.
    pub fn steps(cx: &BookCx<'_>) -> Result<ById<UnitType, MoveStep>, BookError> {
        let steps = cx.fact_if::<Self>()?;
        Ok(steps.map_or_else(ById::default, |steps| steps.steps.clone()))
    }

    /// Each unit type's acceleration step, as the module of the stats they are of makes them;
    /// none in a match without that module.
    pub fn accelerations(cx: &BookCx<'_>) -> Result<ById<UnitType, AccelStep>, BookError> {
        let steps = cx.fact_if::<Self>()?;
        Ok(steps.map_or_else(ById::default, |steps| steps.accelerations.clone()))
    }

    /// Each unit type's turn step, as the module of the stats they are of makes them; none in a
    /// match without that module.
    pub fn turns(cx: &BookCx<'_>) -> Result<ById<UnitType, TurnStep>, BookError> {
        let steps = cx.fact_if::<Self>()?;
        Ok(steps.map_or_else(ById::default, |steps| steps.turns.clone()))
    }
}
