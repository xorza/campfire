use campfire_actions::LoadedAction;
use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_script::UnknownName;
use campfire_values::{ActionId, Named, TypeScope, UnitType};

use crate::train_data::TrainData;

/// What a train action runs: the unit type it makes, whose spawn draws its variation when it varies
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
}

impl PerId<ActionId> for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl TrainSpec {
    /// What each train makes: a unit type of its package; an error for one it does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<TrainData>() else {
                continue;
            };
            let scope = TypeScope::of_package(action.package);
            let resolve = |name: &DeclaredName| {
                let unit_type = cx.unit_type(scope, name.as_str());
                unit_type.ok_or_else(|| action.unknown(UnknownName::new(UnitType::NAMES, name)))
            };
            let spec = Self {
                unit_type: resolve(&data.unit_type)?,
            };
            book.set(action.action, spec);
        }
        Ok(book.build())
    }
}
