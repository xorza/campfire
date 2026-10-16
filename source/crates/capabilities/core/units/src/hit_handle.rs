use campfire_script::{ApiBuilder, ApiOwner, MemberSpec, ScriptValue, View};
use campfire_sim::Capability;
use campfire_values::Hit;

/// A hit as a script holds it, `Hit` in scripts: read only.
#[derive(Debug, Clone)]
pub struct HitHandle {
    hit: Hit,
    view: View,
}

impl HitHandle {
    pub const fn new(hit: Hit, view: View) -> Self {
        Self { hit, view }
    }

    /// The `Hit` handle's fields.
    pub fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Hit, name, description).capability(Capability::Abilities)
        };
        let unit = |hit: &Self, id| hit.view.unit_value(id);
        api.ty::<Self>("Hit")
            .bind(
                field(
                    "delivery",
                    "the projectile or area unit that delivered it, `()` when at once or gone",
                ),
                move |hit: &mut Self| unit(hit, hit.hit.delivery),
            )
            .bind(
                field("target", "the unit the action aimed at, `()` with none"),
                move |hit: &mut Self| unit(hit, hit.hit.target),
            )
            .bind(
                field("pos", "where it hit, or where its delivery ended"),
                |hit: &mut Self| hit.hit.pos,
            )
            .bind(
                field("distance", "how far its delivery flew"),
                |hit: &mut Self| hit.hit.distance,
            )
            .bind(
                field(
                    "direction",
                    "the direction its delivery flew in, `()` for an area or a delivery that did not move",
                ),
                |hit: &mut Self| hit.hit.direction.map_or(ScriptValue::UNIT, ScriptValue::of),
            )
            .plan(
                MemberSpec::field(
                    ApiOwner::Hit,
                    "part",
                    "the body part a ray or a sweep struck, `()` with none",
                )
                .capability(Capability::Hitboxes),
            );
    }
}
