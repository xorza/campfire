use campfire_actions::{DeliveryData, DeliveryShape};
use campfire_deliveries::Launcher;
use campfire_math::Vec3;
use campfire_script::{ApiBuilder, Checked, Ctx, MemberDecl, RoleSet, ScriptPosition, Unit};

use crate::area_data::{AreaData, AreaInsideData};
use crate::areas_effect::AreasEffect;

/// The script API of `areas`: `ctx.land`, and the data of an area type.
#[derive(Debug)]
pub struct AreasApi;

impl AreasApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let area = MemberDecl::call(
            "land",
            &[&["position"]],
            "lands one more of the action's areas at `position`, its own cast; the new area, which spawns later in the tick",
        )
        .roles(RoleSet::ACTION);
        api.call(area, |ctx: &mut Ctx, position: ScriptPosition| {
            Self::land(ctx, position.get())
        })
        .own_fields::<DeliveryData>(&["area"])
        .data::<AreaData>(&[])
        .data::<AreaInsideData>(&[]);
    }

    /// Queues an area of the running action, which delivers areas, from its acting unit: the new
    /// area, with the id the call takes for it.
    fn land(ctx: &Ctx, position: Vec3) -> Checked<Unit> {
        let launcher = Launcher::of(ctx, |shape| shape == DeliveryShape::Area)?;
        let unit_type = launcher.delivery.unit_type;
        let mut frame = ctx.write()?;
        let id = frame.take_id();
        frame.queue(AreasEffect {
            id: Some(id),
            by: launcher.by,
            unit_type,
            position,
        });
        Ok(Unit::created(id, unit_type, ctx.view().clone()))
    }
}
