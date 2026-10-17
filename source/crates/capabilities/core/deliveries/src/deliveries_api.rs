use campfire_script::{
    ApiBuilder, ApiOwner, ApiVersion, Hook, MemberSpec, PlannedCapability, ScriptValue, Status,
};
use campfire_sim::Capability;
use campfire_units::HitHandle;

/// The script API of the deliveries that projectiles and areas make, where they run: the hit
/// their hooks record, and their `on_hit` and `on_end` hooks.
#[derive(Debug)]
pub(crate) struct DeliveriesApi;

impl DeliveriesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        Self::hit(api);
        api.hook(Hook::OnHit, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnEnd, Status::Runs(ApiVersion::FIRST));
    }

    /// The `Hit` handle's fields.
    fn hit(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Hit, name, description).capability(Capability::Abilities)
        };
        api.ty::<HitHandle>("Hit")
            .bind(
                field(
                    "delivery",
                    "the projectile or area unit that delivered it, `()` when at once or gone",
                ),
                |hit: &mut HitHandle| hit.unit(hit.hit().delivery),
            )
            .bind(
                field("target", "the unit the action aimed at, `()` with none"),
                |hit: &mut HitHandle| hit.unit(hit.hit().target),
            )
            .bind(
                field("pos", "where it hit, or where its delivery ended"),
                |hit: &mut HitHandle| hit.hit().pos,
            )
            .bind(
                field("distance", "how far its delivery flew"),
                |hit: &mut HitHandle| hit.hit().distance,
            )
            .bind(
                field(
                    "direction",
                    "the direction its delivery flew in, `()` for an area or a delivery that did not move",
                ),
                |hit: &mut HitHandle| {
                    let direction = hit.hit().direction;
                    direction.map_or(ScriptValue::UNIT, ScriptValue::of)
                },
            )
            .plan(
                MemberSpec::field(
                    ApiOwner::Hit,
                    "part",
                "the body part a ray or a sweep struck, `()` with none",
            )
            .planned_in(PlannedCapability::Hitboxes),
            );
    }
}
