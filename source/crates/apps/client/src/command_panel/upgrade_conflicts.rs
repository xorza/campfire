use std::collections::BTreeMap;

use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_declarations::ById;
use campfire_stats::TypeUpgrades;
use campfire_values::{ModifierId, UnitType};

/// What refuses a research of a modifier on a unit of a type, by the type's upgrades' conflicts:
/// the triggers of the upgrades the modifier withholds, which refuse it held by the research's own
/// holder, and the conflicts of the upgrades it triggers, which refuse it held by either holder
/// (riders-and-upgrades.md, A conflicting upgrade is refused).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Conflict {
    pub(crate) withheld: Box<[ModifierId]>,
    pub(crate) withholding: Box<[ModifierId]>,
}

/// Each research a type's upgrades' conflicts may refuse, by its type and modifier, with what
/// refuses it, as stats' part of the panel gives them as the client starts; none in a mode of no
/// stats, whose types have no upgrades (02-engine-core.md, A client module list).
#[derive(Resource, Debug, Default)]
pub(crate) struct UpgradeConflicts(BTreeMap<(UnitType, ModifierId), Conflict>);

impl UpgradeConflicts {
    /// What refuses a research of `modifier` on a unit of `unit_type`, none for a research no
    /// conflict refuses.
    pub(crate) fn of(&self, unit_type: UnitType, modifier: ModifierId) -> Option<&Conflict> {
        self.0.get(&(unit_type, modifier))
    }

    /// The conflicts of each modifier an upgrade of `book` names, of each type.
    pub(super) fn of_book(book: &ById<UnitType, TypeUpgrades>) -> Self {
        let mut conflicts = BTreeMap::new();
        for (unit_type, upgrades) in book {
            let named = (upgrades.0.iter())
                .flat_map(|upgrade| upgrade.triggers.iter().chain(&upgrade.conflicts));
            for &modifier in named {
                let conflict = Conflict {
                    withheld: upgrades.withheld(modifier).collect(),
                    withholding: upgrades.withholding(modifier).collect(),
                };
                if !(conflict.withheld.is_empty() && conflict.withholding.is_empty()) {
                    conflicts.insert((unit_type, modifier), conflict);
                }
            }
        }
        Self(conflicts)
    }
}

/// Stats' part of the command panel: the conflicts of its types' upgrades, which hold a research
/// back.
#[derive(Debug)]
pub(crate) struct UpgradeConflictsPlugin;

impl Plugin for UpgradeConflictsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::read);
    }
}

impl UpgradeConflictsPlugin {
    /// Reads the conflicts of the types' upgrades.
    fn read(book: Res<'_, ById<UnitType, TypeUpgrades>>, mut commands: Commands<'_, '_>) {
        commands.insert_resource(UpgradeConflicts::of_book(&book));
    }
}
