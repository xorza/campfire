use campfire_actions::ActionBook;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_names::{Filter, UnknownName};
use campfire_units::UnitTypes;
use campfire_values::{ActionId, Named, Relation, TagSet, UnitType};

use crate::use_data::UseData;

/// An object type's use as a match reads it, package data and not state: the action of the `use`
/// kind a user's use slot runs at it, and the units that may use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseSpec {
    action: ActionId,
    users: Filter,
}

impl UseSpec {
    pub const fn new(action: ActionId, users: Filter) -> Self {
        Self { action, users }
    }

    pub const fn action(self) -> ActionId {
        self.action
    }

    /// Whether its users select a unit of `tags` whose team the object's team regards with
    /// `relation`.
    pub const fn admits(self, relation: Relation, tags: TagSet) -> bool {
        self.users.selects(relation, tags)
    }
}

impl PerId<UnitType> for UseSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl UseSpec {
    /// The use of each object type that stands, its action among its package's and its users
    /// among the match's tags; an error for a name the package or the match does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<UseData>() else {
                continue;
            };
            let spec = || {
                let action = actions.names().named((unit.package, data.action.as_str()));
                let action =
                    action.ok_or_else(|| UnknownName::new(ActionId::NAMES, &data.action))?;
                let users = Filter::resolve(&data.users, tags)?;
                Ok(Self::new(action, users))
            };
            let at = || BookPlace::UnitType(unit.place());
            let spec = spec().map_err(|name| BookError::unknown(unit.package, at(), name))?;
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}
