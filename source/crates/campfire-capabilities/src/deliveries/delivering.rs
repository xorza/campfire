use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::actions::actions_column::ActionsColumn;
use crate::actions::delivery::{Delivery, DeliveryShape};
use crate::actions::effect_lists::LaunchId;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;
use crate::units::action_id::ActionId;

/// What a delivery belongs to: the unit that delivers it, its action at `rank`, and the launch
/// whose lists it runs, none for the action's own delivery, which runs the action's lists and
/// hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Delivering {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
    pub(crate) launch: Option<LaunchId>,
}

/// One more delivery a call launches: what it belongs to, and its action's delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launcher {
    pub(crate) by: Delivering,
    pub(crate) delivery: Delivery,
}

impl Delivering {
    /// The running action, from its acting unit, for a call that launches one more of its
    /// deliveries, and the type it delivers; an error outside an action, or for an action whose
    /// delivery's shape `shape` refuses.
    pub(crate) fn of(ctx: &Ctx, shape: fn(DeliveryShape) -> bool) -> Checked<Launcher> {
        ctx.require(RoleSet::ACTION)?;
        let frame = ctx.frame();
        let (Some(source), Some(action)) = (frame.acting(), frame.action()) else {
            return Err(ApiError::NotForRole.fail().into());
        };
        let rank = frame.rank();
        drop(frame);
        let delivery = ActionsColumn::delivers(ctx.view(), action);
        let Some(delivery) = delivery.filter(|delivery| shape(delivery.shape)) else {
            return Err(ApiError::NoDelivery.fail().into());
        };
        let by = Delivering {
            source,
            action,
            rank,
            launch: None,
        };
        Ok(Launcher { by, delivery })
    }
}
