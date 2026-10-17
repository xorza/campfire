use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{ParamSet, Query};
use campfire_actions::{ActionProgress, ActionSlots, ActionTarget, SlotTimes, SlotsMut};
use campfire_sim::Unpredicted;
use campfire_units::{OrderKind, UnitOrdered};

use crate::using::{UnitUse, Using};

/// A user's slots, their times and their progress, whether a client predicts it or not.
type UserSlots<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut ActionSlots,
        &'static mut SlotTimes,
        &'static mut ActionProgress,
    ),
    Allow<Unpredicted>,
>;

/// The use orders of `interaction`, which `orders` gives.
#[derive(Debug)]
pub(crate) struct UseOrders;

impl UseOrders {
    /// The observer that starts a use as it is ordered: the object's action, in the use slot its
    /// users give the unit, filled afresh unless the slot holds it, ordered as a slot's cast. A
    /// use by a unit with no use slot, or that the object's users do not select, is dropped.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "an observer takes its event and its params by value"
    )]
    pub(crate) fn ordered(
        ordered: On<'_, '_, UnitOrdered>,
        mut units: ParamSet<'_, '_, (Using<'_, '_>, UserSlots<'_, '_>)>,
    ) {
        let (entity, OrderKind::Use { target }) = (ordered.entity, ordered.order) else {
            return;
        };
        let Some(UnitUse { slot, action }) = units.p0().of(entity, target) else {
            return;
        };
        let mut users = units.p1();
        let (mut slots, mut times, mut progress) = users
            .get_mut(entity)
            .expect("a user holds its slots, their times and their progress");
        if slots.slot(slot).and_then(|held| held.action) != Some(action) {
            let mut unit = SlotsMut {
                slots: &mut slots,
                times: &mut times,
                progress: &mut progress,
            };
            unit.fill(slot, Some(action));
        }
        progress.order(slot, ActionTarget::Unit(target));
    }
}
