use campfire_sim::{Capability, Position};

use crate::actions::action_book::Delivery;
use crate::areas::area_effect::AreaEffect;
use crate::deliveries::delivering::Delivering;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::Checked;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{DataTable, MemberSpec};

/// The script API of `areas`: `ctx.area`, and the data of an area type.
#[derive(Debug)]
pub(crate) struct AreasApi;

impl AreasApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let area = MemberSpec::call(
            "area",
            "(pos)",
            "lands one more of the action's areas at `pos`, its own cast",
        )
        .roles(RoleSet::ACTION)
        .capability(Capability::Areas);
        api.bind(area, |ctx: &mut Ctx, at: Position| AreasApi::land(ctx, at))
            .data(DataTable::Delivery, &["area"], &[])
            .data(
                DataTable::Area,
                &["radius", "delay_ms", "duration_ms", "affects", "inside"],
                &[],
            )
            .data(DataTable::AreaInside, &["self", "allies", "enemies"], &[]);
    }

    /// Queues an area of the running action, which delivers areas, from its acting unit.
    fn land(ctx: &Ctx, at: Position) -> Checked<()> {
        let by = Delivering::of(ctx, |delivery| delivery == Delivery::Area)?;
        ctx.queue(Effect::Area(AreaEffect { by, at }))
    }
}
