use campfire_actions::Flights;
use campfire_declare::{BookCx, BookError, BookPlace, ByType};
use campfire_units::UnitTypes;

use crate::Projectiles;
use crate::projectile_data::ProjectileData;
use crate::projectile_spec::ProjectileSpec;

impl Projectiles {
    /// Tags each projectile type `projectile`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn tag_types(cx: &BookCx<'_>, types: &mut UnitTypes) -> Result<(), BookError> {
        for unit in cx.types() {
            if unit.data.section::<ProjectileData>().is_some() {
                types.give_tag(unit.unit_type, Self::TAG.tag());
            }
        }
        Ok(())
    }

    /// How each projectile type flies.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn flights(cx: &BookCx<'_>) -> Result<Flights, BookError> {
        let mut flights = ByType::default();
        for unit in cx.types() {
            if let Some(projectile) = unit.data.section::<ProjectileData>() {
                flights.set(unit.unit_type, projectile.flight());
            }
        }
        Ok(Flights(flights))
    }
}

impl ProjectileSpec {
    /// The spec of each projectile type, its filters resolved against the match's tags; an error
    /// for a tag the match does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByType::default();
        for unit in cx.types() {
            let Some(projectile) = unit.data.section::<ProjectileData>() else {
                continue;
            };
            let spec = Self::of(projectile, tags, cx.rate()).map_err(|name| {
                BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name)
            })?;
            book.set(unit.unit_type, spec);
        }
        Ok(book)
    }
}
