use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};
use crate::units::block::Block;
use crate::units::tag_effect::TagEffect;

/// The script API of `abilities`, `projectiles` and `areas` that design 08 plans: the values of
/// a cast, the projectiles and areas it makes, and the bookkeeping of cooldowns and charges.
#[derive(Debug)]
pub(crate) struct AbilitiesApi;

impl AbilitiesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let cast = |name, description| {
            MemberSpec::value(name, description)
                .roles(RoleSet::ACTION)
                .capability(Capability::Abilities)
        };
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Abilities)
        };
        api.plan(cast("range", "the ability's range at its rank"))
            .plan(cast("charge", "how long a charged cast was held, from 0 to 1"))
            .plan(cast("origin", "where the cast comes from"))
            .plan(
                MemberSpec::call(
                    "projectile",
                    "(from, to) or (from, to, overrides)",
                    "the ability's projectile, flying a line, homing on a unit or flying to a position",
                )
                .roles(RoleSet::ACTION)
                .capability(Capability::Projectiles),
            )
            .plan(
                MemberSpec::call("area", "(pos)", "the ability's area at `pos`")
                    .roles(RoleSet::ACTION)
                    .capability(Capability::Areas),
            )
            .plan(call(
                "reduce_cooldown",
                "(unit, id, ms)",
                "takes `ms` off the cooldown of `unit`'s ability `id`",
            ))
            .plan(call(
                "reduce_cooldowns",
                "(unit, fraction)",
                "takes `fraction` off the cooldowns of `unit`'s basic abilities",
            ))
            .plan(call("add_charge", "(unit, id)", "gives `unit`'s ability `id` a charge"));
        AbilitiesApi::register_deliveries(api);
        api.tag_effect(TagEffect::Blocks(Block::Cast), Status::Runs)
            .hook(Hook::OnResolve, "(ctx, unit, target)", Status::Runs)
            .hook(Hook::OnHit, "(ctx, unit, target, hit)", Status::Planned)
            .hook(Hook::OnEnd, "(ctx, unit, hit)", Status::Planned)
            .hook(Hook::OnChannelTick, "(ctx, unit)", Status::Planned)
            .hook(Hook::OnInterrupt, "(ctx, unit, target)", Status::Planned)
            .data(
                DataTable::Ability,
                &[
                    "script",
                    "targeting",
                    "range",
                    "cooldown_ms",
                    "cost",
                    "cast_time_ms",
                    "passive_modifier",
                    "passive_while_ready",
                    "params",
                ],
                &[
                    "clamp_to_range",
                    "toggle",
                    "channel",
                    "hold",
                    "charges",
                    "charge",
                    "projectile",
                    "area",
                    "projectile_state",
                ],
            );
    }

    /// The planned handles of deliveries: a projectile, an area, and the hit either records.
    fn register_deliveries(api: &mut ApiBuilder<'_>) {
        let projectile = |name, description| {
            MemberSpec::field(ApiOwner::Projectile, name, description)
                .capability(Capability::Projectiles)
        };
        api.plan(projectile("source", "the unit that launched it"))
            .plan(projectile("pos", "where it flies"))
            .plan(projectile("distance", "how far it flew"))
            .plan(projectile(
                "state",
                "its script state, which a call may write",
            ));
        let hit = |name, description| {
            MemberSpec::field(ApiOwner::Hit, name, description).capability(Capability::Abilities)
        };
        let area = |name, description| {
            MemberSpec::field(ApiOwner::Area, name, description).capability(Capability::Areas)
        };
        api.plan(area("source", "the unit that made it"))
            .plan(area("pos", "where it lies"))
            .plan(hit(
                "delivery",
                "the projectile or area unit that delivered it, `()` when at once",
            ))
            .plan(hit(
                "target",
                "the unit the action aimed at, `()` with none",
            ))
            .plan(hit("pos", "where it hit, or where its delivery ended"))
            .plan(hit("distance", "how far its delivery flew"))
            .plan(hit("direction", "the direction it came from"))
            .plan(hit(
                "part",
                "the body part a ray or a sweep struck, `()` with none",
            ));
    }
}
