use campfire_actions::LoadedAction;
use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_names::UnknownName;
use campfire_values::{ActionId, Named, TypeScope, UnitType};

use crate::train_data::TrainData;

/// What a train action runs: the unit type it makes, whose spawn draws its variation when it varies
/// (04-capabilities/00-overview.md, A unit type may vary its units), and whether it makes it at a
/// place of its producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
    pub at_place: bool,
}

impl PerId<ActionId> for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl TrainSpec {
    /// What each train makes: a unit type of its package; an error for one it does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            let spec = Self::of(&action, |scope, name| cx.unit_type(scope, name))?;
            if let Some(spec) = spec {
                book.set(action.action, spec);
            }
        }
        Ok(book.build())
    }

    /// What `action` trains, its unit type by `unit_type`'s lookup of a name in a scope, when it
    /// is a train; an error for a unit type of its package that is none.
    pub(crate) fn of(
        action: &LoadedAction<'_>,
        unit_type: impl Fn(TypeScope, &str) -> Option<UnitType>,
    ) -> Result<Option<Self>, BookError> {
        let Some(data) = action.section::<TrainData>() else {
            return Ok(None);
        };
        let scope = TypeScope::of_package(action.package);
        let name = &data.unit_type;
        let unit_type = unit_type(scope, name.as_str())
            .ok_or_else(|| action.unknown(UnknownName::new(UnitType::NAMES, name)))?;
        Ok(Some(Self {
            unit_type,
            at_place: data.at_place,
        }))
    }
}
