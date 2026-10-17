use campfire_actions::{ActionKind, ActionsData, SlotClaim, SlotClaims, Targeting, TypeSlotsData};
use campfire_declare::{BookCx, BookError, BookPlace, Section, SourceKind};
use campfire_sim::Capability;

use crate::Interaction;
use crate::error::UseError;
use crate::use_data::UseData;
use crate::uses_data::UsesData;

impl Interaction {
    /// Claims the use slot of each unit type that holds uses, of its slot kind, as its objects'
    /// uses fill it.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn claim_slots(cx: &BookCx<'_>, claims: &mut SlotClaims) -> Result<(), BookError> {
        for unit in cx.types() {
            if let Some(uses) = unit.data.section::<UsesData>() {
                claims.0.push(SlotClaim {
                    unit_type: unit.unit_type,
                    kind: uses.kind.clone(),
                    slots: 1,
                    by: Capability::Interaction,
                });
            }
        }
        Ok(())
    }

    /// The rule of the uses: each `use` section names an action of its package of the `use`
    /// kind, no unit type's slots and no loadout name one, as a use runs only from its object,
    /// and each use aims at a unit its filter selects. A name the package does not have is the
    /// books' problem.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let kind = |package, id| {
            let actions = cx.package(package).content.get::<ActionsData>();
            let action = actions.and_then(|actions| actions.0.get(id));
            action.map(|action| action.kind)
        };
        for unit in cx.types() {
            let fail = |problem| {
                let at = BookPlace::UnitType(unit.place());
                Err(BookError::rule(unit.package, at, problem))
            };
            if let Some(usable) = unit.data.section::<UseData>() {
                let used = kind(unit.package, &usable.action);
                if used.is_some_and(|used| used != ActionKind::Use) {
                    return fail(UseError::NotUse(usable.action.clone()));
                }
            }
            let slots = unit.data.section::<TypeSlotsData>().into_iter();
            let mut slotted = slots.flat_map(|slots| slots.0.values()).flatten();
            let named = slotted.find(|id| kind(unit.package, id) == Some(ActionKind::Use));
            if let Some(id) = named {
                return fail(UseError::Named(id.clone()));
            }
        }
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            let mut actions = actions.flat_map(|actions| &actions.0);
            let aimless = actions.find(|(_, action)| {
                action.kind == ActionKind::Use && !matches!(action.targeting, Targeting::Unit(_))
            });
            if let Some((id, _)) = aimless {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                return Err(BookError::rule(package, at, UseError::Aims));
            }
        }
        for (package, source) in cx.packages() {
            if !matches!(source.kind, SourceKind::Loadout) {
                continue;
            }
            let actions = source.content.get::<ActionsData>().into_iter();
            let mut actions = actions.flat_map(|actions| &actions.0);
            if let Some((id, _)) = actions.find(|(_, action)| action.kind == ActionKind::Use) {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                return Err(BookError::rule(package, at, UseError::Named(id.clone())));
            }
        }
        Ok(())
    }
}
