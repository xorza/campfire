use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};

/// The script API of `abilities`, `projectiles` and `areas` that design 08 plans: the values of
/// a cast, the projectiles and areas it makes, and the bookkeeping of cooldowns and charges.
#[derive(Debug)]
pub(crate) struct AbilitiesApi;

impl AbilitiesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let cast = |name, description| {
            MemberSpec::value(name, description)
                .roles(RoleSet::ABILITY)
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
                .roles(RoleSet::ABILITY)
                .capability(Capability::Projectiles),
            )
            .plan(
                MemberSpec::call("area", "(pos)", "the ability's area at `pos`")
                    .roles(RoleSet::ABILITY)
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
        let area = |name, description| {
            MemberSpec::field(ApiOwner::Area, name, description).capability(Capability::Areas)
        };
        api.plan(area("source", "the unit that made it"))
            .plan(area("pos", "where it lies"))
            .hook(Hook::OnCast, "(ctx, caster, target)", Status::Runs)
            .hook(Hook::OnChannelTick, "(ctx, caster)", Status::Planned)
            .hook(
                Hook::OnProjectileHit,
                "(ctx, proj, target)",
                Status::Planned,
            )
            .hook(Hook::OnProjectileEnd, "(ctx, proj)", Status::Planned)
            .hook(Hook::OnAreaTrigger, "(ctx, area, units)", Status::Planned)
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
}
