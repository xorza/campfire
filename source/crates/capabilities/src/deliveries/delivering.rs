use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::actions::action_book::{ActionId, DeliveryShape};
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;
use crate::units::unit_type::UnitType;

/// What a delivery belongs to: the unit that delivers it, and its action at `rank`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Delivering {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
}

/// One more delivery a call launches: what it belongs to, and the unit type it delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launcher {
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
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
        let delivery = ctx.view().delivers(action);
        let Some(delivery) = delivery.filter(|delivery| shape(delivery.shape)) else {
            return Err(ApiError::NoDelivery.fail().into());
        };
        let by = Delivering {
            source,
            action,
            rank,
        };
        Ok(Launcher {
            by,
            unit_type: delivery.unit_type,
        })
    }
}
