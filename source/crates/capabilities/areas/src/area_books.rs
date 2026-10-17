use campfire_actions::{ActionsData, DeliveryData, DoesData, LaunchFields, Targeting};
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, ByType, Section};
use campfire_script::{NameKind, UnknownName};
use campfire_stats::ModifierBook;
use campfire_units::UnitTypes;

use crate::Areas;
use crate::area_data::AreaData;
use crate::area_spec::AreaSpec;
use crate::error::{AreaDeliveryError, AreaError};

impl Areas {
    /// The rule of every action's area delivery and launches: each of a unit type of the area
    /// section, and a delivery aimed at other than along a direction. A type the package does not
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
                let delivered = match &action.delivery {
                    Some(DeliveryData::Area { unit_type }) => Some(unit_type),
                    Some(DeliveryData::Projectile { .. }) | None => None,
                };
                let launched = action.effects().filter_map(|effect| match &effect.does {
                    DoesData::Launch(LaunchFields { area, .. }) => Some(area),
                    _ => None,
                });
                if let Some(name) = delivered.into_iter().chain(launched).find(not_area) {
                    return fail(AreaDeliveryError::NotArea(name.clone()));
                }
                if delivered.is_some() && action.targeting == Targeting::Direction {
                    return fail(AreaDeliveryError::Direction);
                }
            }
        }
        Ok(())
    }

    /// Tags each area type `area`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn tag_types(cx: &BookCx<'_>, types: &mut UnitTypes) -> Result<(), BookError> {
        for unit in cx.types() {
            if unit.data.section::<AreaData>().is_some() {
                types.give_tag(unit.unit_type, Self::TAG.tag());
            }
        }
        Ok(())
    }
}

impl AreaSpec {
    /// The spec of each area type, its filters resolved against the match's tags and its modifiers
    /// among its package's; an error for a name the match does not have, or a time too large to
    /// count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let modifiers = cx.book::<ModifierBook>()?;
        let mut book = ByType::default();
        for unit in cx.types() {
            let Some(area) = unit.data.section::<AreaData>() else {
                continue;
            };
            let modifier = |name: &DeclaredName| {
                let modifier = modifiers.names().named((unit.package, name.as_str()));
                modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
            };
            let at = || BookPlace::UnitType(unit.place());
            let spec = Self::of(area, tags, cx.rate(), modifier).map_err(|error| match error {
                AreaError::Time(error) => BookError::rule(unit.package, at(), error),
                AreaError::Unknown(name) => BookError::unknown(unit.package, at(), name),
            })?;
            book.set(unit.unit_type, spec);
        }
        Ok(book)
    }
}
