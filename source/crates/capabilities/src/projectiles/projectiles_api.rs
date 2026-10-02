use campfire_math::Vec3;
use campfire_sim::{Capability, Position};

use crate::actions::action_book::Delivery;
use crate::deliveries::delivering::Delivering;
use crate::projectiles::projectile_effect::{ProjectileEffect, Toward};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{DataTable, MemberSpec};
use crate::units::unit::Unit;

/// The script API of `projectiles`: `ctx.projectile`, and the data of a projectile type.
#[derive(Debug)]
pub(crate) struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let projectile = MemberSpec::call(
            "projectile",
            "(from, direction) or (from, unit)",
            "launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, or homing on `unit` for a homing type",
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
    /// unit; an error for a form its type does not fly in.
    fn launch(ctx: &Ctx, from: Position, toward: Toward) -> Checked<()> {
        let by = Delivering::of(ctx, |delivery| matches!(delivery, Delivery::Projectile(_)))?;
        if ctx.view().launches_homing(by.action) != matches!(toward, Toward::Unit(_)) {
            return Err(ApiError::OtherFlight.fail().into());
        }
        ctx.queue(Effect::Projectile(ProjectileEffect { by, from, toward }))
    }
}
