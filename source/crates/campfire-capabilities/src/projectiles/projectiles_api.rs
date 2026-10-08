use campfire_math::Vec3;
use campfire_sim::{Capability, Position};

use crate::actions::action_data_field::ActionDataField;
use crate::actions::delivery::DeliveryShape;
use crate::deliveries::delivering::Delivering;
use crate::projectiles::projectiles_effect::{ProjectilesEffect, Toward};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::new_unit::NewUnit;
use crate::units::unit::Unit;

/// The script API of `projectiles`: `ctx.projectile`, and the data of a projectile type.
#[derive(Debug)]
pub(crate) struct ProjectilesApi;

impl ProjectilesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let projectile = MemberSpec::call(
            "projectile",
            &[&["from", "direction"], &["from", "unit"]],
            "launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, or homing on `unit` for a homing type; the new projectile, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION)
        .capability(Capability::Projectiles);
        api.bind_for(
            projectile,
            |ctx: &mut Ctx, from: Position, direction: Vec3| {
                ProjectilesApi::launch(ctx, from, Toward::Direction(direction))
            },
        )
        .bind_for(projectile, |ctx: &mut Ctx, from: Position, unit: Unit| {
            ProjectilesApi::launch(ctx, from, Toward::Unit(unit.id))
        })
        .action_fields(ActionDataField::of(Some(Capability::Projectiles)))
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
            &["gravity"],
        );
    }

    /// Queues a projectile of the running action, which delivers projectiles, from its acting
    /// unit: the new projectile, with the id the call takes for it; an error for a form its type
    /// does not fly in.
    fn launch(ctx: &Ctx, from: Position, toward: Toward) -> Checked<NewUnit> {
        let launcher = Delivering::of(ctx, |shape| {
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
        frame.effects.push(ProjectilesEffect {
            id,
            by: launcher.by,
            unit_type,
            from,
            toward,
        });
        Ok(NewUnit { id, unit_type })
    }
}
