use campfire_script::rhai::Dynamic;
use campfire_sim::Capability;

use crate::deliveries::hit::Hit;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::units::script_view::View;

/// A hit as a script holds it, `Hit` in scripts: read only.
#[derive(Debug, Clone)]
pub(crate) struct HitHandle {
    hit: Hit,
    view: View,
}

impl HitHandle {
    pub(crate) const fn new(hit: Hit, view: View) -> HitHandle {
        HitHandle { hit, view }
    }

    /// The `Hit` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Hit, name, description).capability(Capability::Abilities)
        };
        let unit = |hit: &HitHandle, id: Option<_>| {
            id.and_then(|id| hit.view.unit(id))
                .map_or(Dynamic::UNIT, Dynamic::from)
        };
        api.ty::<HitHandle>("Hit")
            .bind(
                field(
                    "delivery",
                    "the projectile or area unit that delivered it, `()` when at once or gone",
                ),
                move |hit: &mut HitHandle| unit(hit, hit.hit.delivery),
            )
            .bind(
                field("target", "the unit the action aimed at, `()` with none"),
                move |hit: &mut HitHandle| unit(hit, hit.hit.target),
            )
            .bind(
                field("pos", "where it hit, or where its delivery ended"),
                |hit: &mut HitHandle| hit.hit.pos,
            )
            .bind(
                field("distance", "how far its delivery flew"),
                |hit: &mut HitHandle| hit.hit.distance,
            )
            .bind(
                field("direction", "the direction its delivery flew in"),
                |hit: &mut HitHandle| hit.hit.direction,
            )
            .plan(
                MemberSpec::field(
                    ApiOwner::Hit,
                    "part",
                    "the body part a ray or a sweep struck, `()` with none",
                )
                .capability(Capability::Hitscan),
            );
    }
}
