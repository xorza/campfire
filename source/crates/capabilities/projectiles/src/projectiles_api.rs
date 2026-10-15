use campfire_actions::{ActionDataField, DeliveryData, DeliveryShape};
use campfire_deliveries::Launcher;
use campfire_math::Vec3;
use campfire_script::{ApiBuilder, ApiError, Checked, Ctx, DataTable, MemberSpec, RoleSet, Unit};
use campfire_sim::{Capability, Position};
use campfire_units::NewUnit;

use crate::projectile_data::ProjectileData;
use crate::projectiles_effect::{ProjectilesEffect, Toward};

/// The script API of `projectiles`: `ctx.projectile`, and the data of a projectile type.
#[derive(Debug)]
pub struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let projectile = MemberSpec::call(
            "projectile",
            &[&["from", "direction"], &["from", "unit"]],
            "launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, or homing on `unit` for a homing type; the new projectile, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION);
        api.call(
            projectile,
            |ctx: &mut Ctx, from: Position, direction: Vec3| {
                ProjectilesApi::launch(ctx, from, Toward::Direction(direction))
            },
        )
        .call(projectile, |ctx: &mut Ctx, from: Position, unit: Unit| {
            ProjectilesApi::launch(ctx, from, Toward::Unit(unit.id))
        })
        .action_fields(ActionDataField::of(Some(Capability::Projectiles)))
        .data::<DeliveryData>(&["projectile", "count", "spread_deg"], &[])
        .shared_data(DataTable::UnitType, &["projectile"], &[])
        .data::<ProjectileData>(
            &[
                "speed",
                "width",
                "range",
                "homing",
                "stop_on_hit",
                "once_per_cast",
                "hits",
            ],
            &["gravity"],
        );
    }

    /// Queues a projectile of the running action, which delivers projectiles, from its acting
    /// unit: the new projectile, with the id the call takes for it; an error for a form its type
    /// does not fly in.
    fn launch(ctx: &Ctx, from: Position, toward: Toward) -> Checked<NewUnit> {
        let launcher = Launcher::of(ctx, |shape| {
            matches!(shape, DeliveryShape::Projectile { .. })
        })?;
        let homes = matches!(
            launcher.delivery.shape,
            DeliveryShape::Projectile { homes: true, .. }
        );
        if homes != matches!(toward, Toward::Unit(_)) {
            return Err(ApiError::OtherFlight.fail().into());
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
        Ok(NewUnit { id, unit_type })
    }
}
