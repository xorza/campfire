use campfire_actions::{ActionsColumn, Delivery, DeliveryShape, LaunchId};
use campfire_core::{ActionId, ApiError, Checked, Ctx};
use campfire_sim::StableId;
use campfire_values::{ActionStart, Rank};
use serde::{Deserialize, Serialize};

/// What a delivery belongs to: the unit that delivers it, its action at `rank` as it started,
/// none for an attack's, and the launch whose lists it runs, none for the action's own delivery,
/// which runs the action's lists and hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivering {
    pub source: StableId,
    pub action: ActionId,
    pub rank: Rank,
    pub start: Option<ActionStart>,
    pub launch: Option<LaunchId>,
}

/// One more delivery a call launches: what it belongs to, and its action's delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launcher {
    pub by: Delivering,
    pub delivery: Delivery,
}

impl Delivering {
    /// The running action, from its acting unit, for an action's call that launches one more of
    /// its deliveries, and the type it delivers; an error for an action whose delivery's shape
    /// `shape` refuses.
    pub fn of(ctx: &Ctx, shape: fn(DeliveryShape) -> bool) -> Checked<Launcher> {
        let frame = ctx.frame();
        let source = frame.acting().expect("an action's call has its unit");
        let action = frame.action().expect("an action's call has its action");
        let (rank, start) = (frame.rank(), frame.start());
        drop(frame);
        let delivery = ActionsColumn::delivers(ctx.view(), action);
        let Some(delivery) = delivery.filter(|delivery| shape(delivery.shape)) else {
            return Err(ApiError::NoDelivery.fail().into());
        };
        let by = Delivering {
            source,
            action,
            rank,
            start,
            launch: None,
        };
        Ok(Launcher { by, delivery })
    }
}
