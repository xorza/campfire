use campfire_sim::Capability;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};
use crate::units::block::Block;
use crate::units::hit_handle::HitHandle;
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
            .plan(cast(
                "charge",
                "how long a charged cast was held, from 0 to 1",
            ))
            .plan(cast("origin", "where the cast comes from"))
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
            .plan(call(
                "add_charge",
                "(unit, id)",
                "gives `unit`'s ability `id` a charge",
            ));
        AbilitiesApi::register_deliveries(api);
        api.tag_effect(TagEffect::Blocks(Block::Cast), Status::Runs)
            .hook(Hook::OnResolve, "(ctx, unit, target)", Status::Runs)
            .hook(Hook::OnHit, "(ctx, unit, target, hit)", Status::Runs)
            .hook(Hook::OnEnd, "(ctx, unit, hit)", Status::Runs)
            .hook(Hook::OnChannelTick, "(ctx, unit)", Status::Planned)
            .hook(Hook::OnInterrupt, "(ctx, unit, target)", Status::Planned)
            .data(
                DataTable::Action,
                &[
                    "script",
                    "cooldown_ms",
                    "params",
                    "on_resolve",
                    "on_hit",
                    "on_end",
                ],
                &[
                    "clamp_to_range",
                    "toggle",
                    "channel",
                    "hold",
                    "charges",
                    "charge",
                    "projectile_state",
                ],
            )
            .data(
                DataTable::Effect,
                &["damage", "heal", "restore", "modifier", "xp", "to"],
                &["purge", "spawn", "launch", "move", "loot", "noise"],
            );
    }

    /// The handles of deliveries: the hit either records, and the planned projectile and area.
    fn register_deliveries(api: &mut ApiBuilder<'_>) {
        HitHandle::register(api);
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
            .plan(area("pos", "where it lies"));
    }
}
