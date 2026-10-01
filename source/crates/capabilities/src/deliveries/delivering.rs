use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::actions::action_book::{ActionId, Delivery};
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::role_set::RoleSet;

/// What a delivery belongs to: the unit that delivers it, and its action at `rank`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Delivering {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
}

impl Delivering {
    /// The running action, from its acting unit, for a call that launches one more of its
    /// deliveries; an error outside an action, or for an action that `delivers` refuses.
    pub(crate) fn of(ctx: &Ctx, delivers: fn(Delivery) -> bool) -> Checked<Delivering> {
        ctx.require(RoleSet::ACTION)?;
        let frame = ctx.frame();
        let (Some(source), Some(action)) = (frame.acting(), frame.action()) else {
            return Err(ApiError::NotForRole.fail().into());
        };
        let rank = frame.rank();
        drop(frame);
        if !ctx.view().delivers(action).is_some_and(delivers) {
            return Err(ApiError::NoDelivery.fail().into());
        }
        Ok(Delivering {
            source,
            action,
            rank,
        })
    }
}
