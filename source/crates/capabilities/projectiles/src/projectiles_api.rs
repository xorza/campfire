use campfire_actions::{DeliveryData, DeliveryShape};
use campfire_deliveries::Launcher;
use campfire_math::Vec3;
use campfire_script::{
    ApiBuilder, CapabilityRefusal, Checked, Ctx, MemberSpec, RoleSet, ScriptPosition, Status, Unit,
};

use crate::error::ProjectilesRefusal;
use crate::projectile_data::ProjectileData;
use crate::projectiles_effect::{ProjectilesEffect, Toward};

/// The script API of `projectiles`: `ctx.launch`, and the data of a projectile type.
#[derive(Debug)]
pub struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let projectile = MemberSpec::call(
            "launch",
            &[&["from", "direction"], &["from", "unit"]],
            "launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, homing on `unit` for a homing type, or on a curve to `unit`'s center for a curve type; the new projectile, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION);
        api.call(
            projectile,
            |ctx: &mut Ctx, from: ScriptPosition, direction: Vec3| {
                Self::launch(ctx, from.get(), Toward::Direction(direction))
            },
        )
        .call(
            projectile,
            |ctx: &mut Ctx, from: ScriptPosition, unit: Unit| {
                Self::launch(ctx, from.get(), Toward::Unit(unit.id))
            },
        )
        .own_fields::<DeliveryData>(&["projectile", "count", "spread_deg"])
        .data::<ProjectileData>(&[("gravity", Status::Planned)]);
    }

    /// Queues a projectile of the running action, which delivers projectiles, from its acting
    /// unit: the new projectile, with the id the call takes for it; an error for a form its type
    /// does not fly in.
    fn launch(ctx: &Ctx, from: Vec3, toward: Toward) -> Checked<Unit> {
        let launcher = Launcher::of(ctx, |shape| {
            matches!(shape, DeliveryShape::Projectile { .. })
        })?;
        let DeliveryShape::Projectile { flight, .. } = launcher.delivery.shape else {
            panic!("a launcher of projectiles");
        };
        if flight.at_unit() != matches!(toward, Toward::Unit(_)) {
            return Err(ProjectilesRefusal::OtherFlight.fail().into());
        }
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
