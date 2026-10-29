use campfire_deliveries::Launcher;
use campfire_math::Vec3;
use campfire_script::{
    ApiBuilder, CapabilityRefusal, Checked, Ctx, MemberDecl, RoleSet, ScriptPosition, Status, Unit,
};
use campfire_sim::{Capability, StableId};

use crate::error::ProjectilesRefusal;
use crate::projectile_data::ProjectileData;
use crate::projectile_delivery_data::ProjectileDeliveryData;
use crate::projectiles_effect::{ProjectilesEffect, Toward};

/// What a script's `ctx.launch` aims at: a vector, or a unit.
#[derive(Debug, Clone, Copy)]
enum LaunchTo {
    Vector(Vec3),
    Unit(StableId),
}

/// The script API of `projectiles`: `ctx.launch`, and the data of a projectile type.
#[derive(Debug)]
pub struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let projectile = MemberDecl::call(
            "launch",
            &[&["from", "to"], &["from", "unit"]],
            "launches one more of the action's projectiles from `from`, its own cast: along the direction `to` for a line type, homing on `unit` for a homing type, or on a curve or steered to `unit`'s center or to the point `to` for a curve or a steered type; the new projectile, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION);
        api.call(
            projectile,
            |ctx: &mut Ctx, from: ScriptPosition, to: Vec3| {
                Self::launch(ctx, from.get(), LaunchTo::Vector(to))
            },
        )
        .call(
            projectile,
            |ctx: &mut Ctx, from: ScriptPosition, unit: Unit| {
                Self::launch(ctx, from.get(), LaunchTo::Unit(unit.id))
            },
        )
        .own_fields::<ProjectileDeliveryData>(&["count", "spread_deg"])
        .data::<ProjectileData>(&[("gravity", Status::Planned)]);
    }

    /// Queues a projectile of the running action, which delivers projectiles, from its acting
    /// unit: the new projectile, with the id the call takes for it; an error for a form its type
    /// does not fly in. A vector is a line's direction, and the point a curve or a steered
    /// flight flies to.
    fn launch(ctx: &Ctx, from: Vec3, to: LaunchTo) -> Checked<Unit> {
        let launcher = Launcher::of(ctx, Capability::Projectiles)?;
        let aim = launcher.delivery.aim;
        let toward = match to {
            LaunchTo::Unit(unit) if aim.is_at_unit() => Toward::Unit(unit),
            LaunchTo::Vector(point) if aim.flies_to_point() => Toward::Point(point),
            LaunchTo::Vector(direction) if !aim.is_at_unit() => Toward::Direction(direction),
            LaunchTo::Unit(_) | LaunchTo::Vector(_) => {
                return Err(ProjectilesRefusal::OtherFlight.fail().into());
            }
        };
        let unit_type = launcher.delivery.unit_type;
        let mut frame = ctx.write()?;
        let id = frame.take_id();
        frame.queue(ProjectilesEffect {
            id,
            by: launcher.by,
            unit_type,
            from,
            toward,
        });
        Ok(Unit::created(id, unit_type, ctx.view().clone()))
    }
}
