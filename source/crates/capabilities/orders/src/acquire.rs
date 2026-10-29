use campfire_actions::{ActionBook, ActionKind, ActionSlots};
use campfire_common::Ticks;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_names::Filters;
use campfire_units::UnitTypes;
use campfire_values::{TimeTooLarge, UnitType};

use crate::acquire_data::AcquireData;
use crate::error::AcquireError;
use crate::next_turn::{NextTurn, TurnSpec};

/// A unit type's idle scan as a match runs it: its period in whole ticks, at least one, and the
/// units it may take as its target (04-capabilities/control.md, An idle unit scans for a target,
/// as its type declares).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acquire {
    pub(crate) period: Ticks,
    pub(crate) targets: Filters,
}

impl Acquire {
    /// The scan of each unit type that stands and declares one, its filters resolved against the
    /// match's tags; an error for a period too long to count, no filter, a tag the match lacks, or
    /// a type with no weapon or one that builds or gathers
    /// (04-capabilities/control.md, A unit that scans neither builds nor gathers).
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let actions = cx.book::<ActionBook>()?;
        let slots = cx.book::<ById<UnitType, ActionSlots>>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<AcquireData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let rule = |error| BookError::rule(unit.package, at(), error);
            let period = cx.rate().duration(data.scan_ms);
            let period = period.ok_or_else(|| rule(AcquireError::Time(TimeTooLarge)))?;
            if data.targets.is_empty() {
                return Err(rule(AcquireError::NoTargets));
            }
            let held = slots.get(unit.unit_type);
            if held.is_none_or(|held| !held.holds_weapon(actions)) {
                return Err(rule(AcquireError::NoWeapon));
            }
            let works = held.is_some_and(|held| {
                held.indexed().any(|(at, _)| {
                    let kind = held.kind_in(actions, at);
                    matches!(kind, Some(ActionKind::Build | ActionKind::Gather))
                })
            });
            if works {
                return Err(rule(AcquireError::Works));
            }
            let targets = Filters::resolve(&data.targets, tags)
                .map_err(|name| BookError::unknown(unit.package, at(), name))?;
            book.set(unit.unit_type, Self { period, targets });
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for Acquire {
    const SCRIPTS_ONLY: bool = false;
}

/// The tick a unit that scans is due in again, a part of each unit of a type with an idle scan
/// from its spawn. A scan writes it anew.
pub type NextScan = NextTurn<Acquire>;

impl TurnSpec for Acquire {
    const NAME: &'static str = "orders.next_scan";

    fn period(&self) -> Ticks {
        self.period
    }
}
