use std::mem;

use campfire_actions::{ActionsData, DeliveryAim, DeliveryAims, Targeting};
use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Section, TypePlace};
use campfire_names::UnknownName;
use campfire_stats::{ModifierAppliers, ModifiersData, Way};
use campfire_values::{ModifierId, Named};

use crate::Areas;
use crate::area_data::AreaData;
use crate::area_delivery_data::AreaDeliveryData;
use crate::error::AreaDeliveryError;

impl Areas {
    /// Each area type is placed where the action that delivers it aims.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn aims(cx: &BookCx<'_>, aims: &mut DeliveryAims) -> Result<(), BookError> {
        let mut by_type = mem::take(&mut aims.0).reopen();
        for unit in cx.types() {
            if unit.data.section::<AreaData>().is_some() {
                by_type.set(unit.unit_type, DeliveryAim::Placed);
            }
        }
        aims.0 = by_type.build();
        Ok(())
    }

    /// Adds that each action that delivers an area applies the modifiers the area holds on the
    /// units inside it; an error for one its package does not have, through which no way passes.
    pub(crate) fn inside_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let modifiers = source.content.get::<ModifiersData>();
            let actions = source.content.get::<ActionsData>();
            for (id, action) in actions.into_iter().flat_map(|actions| &actions.0) {
                let Some(AreaDeliveryData { area: unit_type }) = action.delivery() else {
                    continue;
                };
                let area = source.units.get(unit_type);
                let Some(area) = area.and_then(|unit| unit.section::<AreaData>()) else {
                    continue;
                };
                for modifier in area.inside.modifiers() {
                    if !modifiers.is_some_and(|modifiers| modifiers.0.contains_key(modifier)) {
                        let at = BookPlace::UnitType(TypePlace::Declared(unit_type.clone()));
                        let name = UnknownName::new(ModifierId::NAMES, modifier);
                        return Err(BookError::unknown(package, at, name));
                    }
                    appliers.apply(package, modifier.as_str(), Way::Action(id.clone()));
                }
            }
        }
        Ok(())
    }

    /// The rule of every action's area delivery: of a unit type of the area section, aimed at other
    /// than along a direction; a launch's type is its effect's rule. A type the package does not
    /// have is the actions' problem.
    pub(crate) fn deliveries_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let Some(actions) = source.content.get::<ActionsData>() else {
                continue;
            };
            for (id, action) in &actions.0 {
                let fail = |problem| {
                    let at = BookPlace::Entry {
                        table: ActionsData::KEY,
                        name: id.clone(),
                    };
                    Err(BookError::rule(package, at, problem))
                };
                let not_area = |name: &&DeclaredName| {
                    let unit = source.units.get(*name);
                    unit.is_some_and(|unit| unit.section::<AreaData>().is_none())
                };
                let delivered = action
                    .delivery::<AreaDeliveryData>()
                    .map(|delivery| &delivery.area);
                if let Some(name) = delivered.filter(not_area) {
                    return fail(AreaDeliveryError::NotArea(name.clone()));
                }
                if delivered.is_some() && action.keys.targeting == Targeting::Direction {
                    return fail(AreaDeliveryError::Direction);
                }
            }
        }
        Ok(())
    }
}
