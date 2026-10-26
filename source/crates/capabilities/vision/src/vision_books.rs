use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder};
use campfire_values::UnitType;

use crate::sight::Sight;
use crate::vision_data::VisionData;

impl Sight {
    /// The sight of each unit type with a `vision` section, a delivery type's among them.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.types() {
            if let Some(vision) = unit.data.section::<VisionData>() {
                book.set(unit.unit_type, vision.sight);
            }
        }
        Ok(book.build())
    }
}
