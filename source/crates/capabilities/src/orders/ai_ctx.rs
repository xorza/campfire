use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_script::rhai::Engine;
use campfire_sim::StableId;

use crate::scripts::error::{ApiError, Checked};
use crate::units::script_view::View;
use crate::units::unit::Unit;

/// `ctx` in an AI script: the queries, and the orders one call queues for the unit that
/// thinks. An AI controls only its own unit. The orders apply only after the call returns
/// successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone)]
pub(crate) struct AiCtx {
    frame: Rc<RefCell<AiFrame>>,
    view: View,
}

/// The unit that thinks in the running call, and the orders the call queued for it.
#[derive(Debug, Default)]
pub(crate) struct AiFrame {
    unit: Option<StableId>,
    pub(crate) orders: Vec<AiOrder>,
}

/// An order an AI call queued for its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiOrder {
    /// Attack `target`, a living enemy.
    Attack { target: StableId },
    /// Drop the target, and walk the path again.
    FollowLane,
}

impl AiCtx {
    pub(crate) fn new(view: View) -> AiCtx {
        AiCtx {
            frame: Rc::default(),
            view,
        }
    }

    /// Starts a call for `unit`, with no orders yet.
    pub(crate) fn begin(&self, unit: StableId) {
        let mut frame = self.frame();
        frame.unit = Some(unit);
        frame.orders.clear();
    }

    /// The call's frame, borrowed until the guard drops. A call borrows it again, so no guard
    /// may live across a call.
    pub(crate) fn frame(&self) -> RefMut<'_, AiFrame> {
        self.frame.borrow_mut()
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    /// The script API of `think`: the queries, `ctx.order_attack` and `ctx.order_follow_lane`.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<AiCtx>("AiCtx")
            .register_fn(
                "order_attack",
                |ctx: &mut AiCtx, unit: Unit, target: Unit| ctx.order_attack(&unit, &target),
            )
            .register_fn("order_follow_lane", |ctx: &mut AiCtx, unit: Unit| {
                ctx.order(&unit, AiOrder::FollowLane)
            });
        View::register_queries::<AiCtx>(engine, AiCtx::view);
    }

    /// Queues an attack of `unit`, which has an attack, on `target`, a living enemy.
    fn order_attack(&self, unit: &Unit, target: &Unit) -> Checked<()> {
        let (ordered, target) = (unit.row(), target.row());
        if ordered.attack_range.is_none() {
            return Err(ApiError::NoAttack.fail().into());
        }
        if !target.alive || !ordered.team.is_enemy_of(target.team) {
            return Err(ApiError::NotAnEnemy.fail().into());
        }
        self.order(unit, AiOrder::Attack { target: target.id })
    }

    /// Queues `order` for `unit`, which must be the unit that thinks.
    fn order(&self, unit: &Unit, order: AiOrder) -> Checked<()> {
        let mut frame = self.frame();
        if frame.unit != Some(unit.id) {
            return Err(ApiError::OtherUnit.fail().into());
        }
        frame.orders.push(order);
        Ok(())
    }
}
