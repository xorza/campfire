use campfire_declarations::{BookCx, BookError, ByType, Fact};

use crate::move_step::MoveStep;
use crate::turn_step::TurnStep;

/// How far each unit type's units walk and turn a tick at level 1, which the module of the stats
/// they are of makes.
#[derive(Debug, Default)]
pub struct UnitSteps {
    pub steps: ByType<MoveStep>,
    pub turns: ByType<TurnStep>,
}

impl Fact for UnitSteps {}

impl UnitSteps {
    /// Each unit type's move step, as the module of the stats they are of makes them; none in a
    /// match without that module.
    pub fn steps(cx: &BookCx<'_>) -> Result<ByType<MoveStep>, BookError> {
        let steps = cx.fact_if::<Self>()?;
        Ok(steps.map_or_else(ByType::default, |steps| steps.steps.clone()))
    }

    /// Each unit type's turn step, as the module of the stats they are of makes them; none in a
    /// match without that module.
    pub fn turns(cx: &BookCx<'_>) -> Result<ByType<TurnStep>, BookError> {
        let steps = cx.fact_if::<Self>()?;
        Ok(steps.map_or_else(ByType::default, |steps| steps.turns.clone()))
    }
}
