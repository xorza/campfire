use campfire_math::Vec3;
use campfire_sim::{Capability, Position};

use crate::projectiles::hit_handle::HitHandle;
use crate::projectiles::projectile_effect::{ProjectileEffect, Toward};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{DataTable, MemberSpec, Status};
use crate::units::unit::Unit;

/// The script API of `projectiles`: `ctx.projectile`, the `Hit` handle, and the hooks of a
/// delivery's hits and end.
#[derive(Debug)]
pub(crate) struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        HitHandle::register(api);
        let projectile = MemberSpec::call(
            "projectile",
            "(from, direction) or (from, unit)",
            "launches one more of the action's projectiles from `from`, along `direction` or homing on `unit`, its own cast",
        )
        .roles(RoleSet::ACTION)
        .capability(Capability::Projectiles);
        api.bind(
            projectile,
            |ctx: &mut Ctx, from: Position, direction: Vec3| {
                ProjectilesApi::launch(ctx, from, Toward::Direction(direction))
            },
        )
        .bind(projectile, |ctx: &mut Ctx, from: Position, unit: Unit| {
            ProjectilesApi::launch(ctx, from, Toward::Unit(unit.id))
        })
        .hook(Hook::OnHit, "(ctx, unit, target, hit)", Status::Runs)
        .hook(Hook::OnEnd, "(ctx, unit, hit)", Status::Runs)
        .data(DataTable::Action, &["delivery"], &[])
        .data(
            DataTable::Delivery,
            &["projectile", "count", "spread_deg"],
            &[],
        )
        .data(
            DataTable::Projectile,
            &[
                "speed",
                "width",
                "range",
                "homing",
                "stop_on_hit",
                "once_per_cast",
                "hits",
            ],
            &["gravity", "sight_radius", "collide"],
        );
    }

    /// Queues a projectile of the running action, which delivers projectiles, from its acting
    /// unit.
    fn launch(ctx: &Ctx, from: Position, toward: Toward) -> Checked<()> {
        ctx.require(RoleSet::ACTION)?;
        let frame = ctx.frame();
        let (Some(source), Some(action)) = (frame.acting(), frame.action()) else {
            return Err(ApiError::NotForRole.fail().into());
        };
        let rank = frame.rank();
        drop(frame);
        if !ctx.view().delivers(action) {
            return Err(ApiError::NoDelivery.fail().into());
        }
        ctx.queue(Effect::Projectile(ProjectileEffect {
            source,
            from,
            action,
            rank,
            toward,
        }))
    }
}
