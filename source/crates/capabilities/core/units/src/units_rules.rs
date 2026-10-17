use campfire_declare::{BookCx, BookError, BookPlace, SectionTable};
use campfire_geometry::Metric;
use campfire_values::EngineTag;

use crate::collision_data::CollisionData;
use crate::error::UnitTypeError;
use crate::map_book::MetricData;
use crate::unit_type_core::TypeTagsData;
use crate::units::Units;

impl Units {
    /// The rule of every unit type: it holds no tag only the engine gives; a delivery type holds
    /// one delivery's section and, beside it, only sections any type holds; and a box body lies
    /// on a planar map.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let metric = cx.map_section::<MetricData>();
        let spatial = metric.is_some_and(|metric| metric.0 == Metric::Spatial);
        let engine = cx.source().engine_tags;
        for unit in cx.types() {
            let fail = |problem| {
                let at = BookPlace::UnitType(unit.place());
                Err(BookError::rule(unit.package, at, problem))
            };
            let mut tags = unit
                .data
                .section::<TypeTagsData>()
                .into_iter()
                .flat_map(|tags| &tags.0);
            if let Some(tag) = tags.find(|tag| EngineTag::named(engine, tag.as_str()).is_some()) {
                return fail(UnitTypeError::EngineTag(tag.clone()));
            }
            if unit.data.delivers() {
                let tables = unit.data.sections().tables();
                let mut delivering = tables
                    .clone()
                    .filter(|&table| table == SectionTable::DeliveryType);
                let other =
                    |table| !matches!(table, SectionTable::DeliveryType | SectionTable::AnyType);
                if delivering.nth(1).is_some() || tables.clone().any(other) {
                    return fail(UnitTypeError::NotDelivery);
                }
            }
            let collision = unit.data.section::<CollisionData>();
            if spatial && collision.is_some_and(|collision| collision.form.is_box()) {
                return fail(UnitTypeError::BoxOnSpatial);
            }
        }
        Ok(())
    }
}
