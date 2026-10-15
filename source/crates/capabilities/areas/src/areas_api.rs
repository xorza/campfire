use campfire_actions::{DeliveryData, DeliveryShape};
use campfire_deliveries::Launcher;
use campfire_script::{ApiBuilder, Checked, Ctx, DataTable, MemberSpec, RoleSet};
use campfire_sim::Position;
use campfire_units::NewUnit;

use crate::area_data::{AreaData, AreaInsideData};
use crate::areas_effect::AreasEffect;

/// The script API of `areas`: `ctx.area`, and the data of an area type.
#[derive(Debug)]
pub struct AreasApi;

impl AreasApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let area = MemberSpec::call(
            "area",
            &[&["pos"]],
            "lands one more of the action's areas at `pos`, its own cast; the new area, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION);
        api.call(area, |ctx: &mut Ctx, pos: Position| {
            AreasApi::land(ctx, pos)
        })
        .data::<DeliveryData>(&["area"], &[])
        .shared_data(DataTable::UnitType, &["area"], &[])
        .data::<AreaData>(
            &["radius", "delay_ms", "duration_ms", "affects", "inside"],
            &[],
        )
        .data::<AreaInsideData>(&["self", "allies", "enemies"], &[]);
    }

    /// Queues an area of the running action, which delivers areas, from its acting unit: the new
    /// area, with the id the call takes for it.
    fn land(ctx: &Ctx, pos: Position) -> Checked<NewUnit> {
        let launcher = Launcher::of(ctx, |shape| shape == DeliveryShape::Area)?;
        let unit_type = launcher.delivery.unit_type;
        let mut frame = ctx.write()?;
        let id = frame.take_id();
        frame.queue(AreasEffect {
            id: Some(id),
            by: launcher.by,
            unit_type,
            pos,
        });
        Ok(NewUnit { id, unit_type })
    }
}
