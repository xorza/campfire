use campfire_actions::{ActionBook, ActionSlots};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_navigation::Walker;
use campfire_values::UnitType;

use crate::error::RetaliationError;
use crate::retaliate_data::RetaliateData;

/// A unit type's `retaliate` as a match runs it: the meters from where its unit stood past which
/// it gives up a chase, and those past which the target lies when it does (combat-response.md, A
/// retaliation's target is chased to its leash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retaliate {
    pub(crate) leash: Num,
    pub(crate) reach: Num,
}

impl PerId<UnitType> for Retaliate {
    const SCRIPTS_ONLY: bool = false;
}

impl Retaliate {
    /// The `retaliate` of each unit type that stands and declares one; an error for a leash or a
    /// reach below 0, or a type of no weapon or that does not walk, as an immobile object does
    /// not answer.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let slots = cx.book::<ById<UnitType, ActionSlots>>()?;
        let walkers = cx.book::<ById<UnitType, Walker>>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<RetaliateData>() else {
                continue;
            };
            let rule =
                |error| BookError::rule(unit.package, BookPlace::UnitType(unit.place()), error);
            if data.leash < Num::ZERO || data.reach < Num::ZERO {
                return Err(rule(RetaliationError::Negative));
            }
            let held = slots.get(unit.unit_type);
            if held.is_none_or(|held| !held.holds_weapon(actions)) {
                return Err(rule(RetaliationError::NoWeapon));
            }
            if walkers.get(unit.unit_type).is_none() {
                return Err(rule(RetaliationError::NoWalk));
            }
            let retaliate = Self {
                leash: data.leash,
                reach: data.reach,
            };
            book.set(unit.unit_type, retaliate);
        }
        Ok(book.build())
    }
}
