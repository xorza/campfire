use campfire_actions::{ActionBook, ActionsData};
use campfire_declare::{BookCx, BookError, Fact, SourceKind};
use campfire_values::{Names, UnitType};

use crate::loadout_setup::LoadoutSetup;
use crate::offer::Offer;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Clone, Default)]
pub struct ModeUnits {
    pub unit_types: Vec<UnitType>,
    pub avatars: Names<Offer>,
    pub loadout: LoadoutSetup,
}

impl Fact for ModeUnits {}

impl ModeUnits {
    /// The mode's unit types that stand, its avatars by their packages' names, and its loadout's
    /// entries, each an action of a loadout package by its id.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let mut units = Self {
            unit_types: cx.standing().map(|unit| unit.unit_type).collect(),
            ..Self::default()
        };
        for (index, package) in cx.packages() {
            match package.kind {
                SourceKind::Avatar(_) => {
                    units.avatars.add(package.name);
                }
                SourceKind::Loadout => {
                    let ids = package.content.get::<ActionsData>().into_iter();
                    for id in ids.flat_map(|actions| actions.0.keys()) {
                        let action = actions.names().named((index, id.as_str()));
                        let action = action.expect("the action book loads each loadout entry");
                        units.loadout.push(id.as_str(), action);
                    }
                }
                SourceKind::Mode | SourceKind::Rules => {}
            }
        }
        Ok(units)
    }
}
