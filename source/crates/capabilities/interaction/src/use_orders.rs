use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{ParamSet, Query, Res};
use campfire_actions::{ActionTarget, ProgressIndexes, SlotsMut};
use campfire_sim::{DueIndex, EntityIndex, StableId, Unpredicted};
use campfire_units::{OrderKind, UnitOrdered};

use crate::riders::Riders;
use crate::using::{UnitUse, Using};

/// A user's slots, their times and their progress, whether a client predicts it or not.
type UserSlots<'w, 's> = Query<'w, 's, (&'static StableId, SlotsMut), Allow<Unpredicted>>;

/// The use orders of `interaction`, which `orders` gives.
#[derive(Debug)]
pub(crate) struct UseOrders;

impl UseOrders {
    /// The observer that starts a use or an entry as it is ordered: the object's action, or the
    /// container's entry, in the use slot its users give the unit, filled afresh unless the slot
    /// holds it, ordered as a slot's cast. One by a unit with no use slot, or that the object's
    /// users or the container's riders do not select, is dropped.
    pub(crate) fn ordered(
        ordered: On<'_, '_, UnitOrdered>,
        mut units: ParamSet<'_, '_, (Using<'_, '_>, UserSlots<'_, '_>)>,
        mut indexes: ProgressIndexes<'_>,
        (mut containers, index): (Query<'_, '_, &mut Riders>, Res<'_, EntityIndex>),
        exits: Option<Res<'_, DueIndex<Riders>>>,
    ) {
        let entity = ordered.entity;
        let (used, target) = match ordered.order {
            OrderKind::Use { target } => (units.p0().of(entity, target), target),
            OrderKind::Enter { target } => (units.p0().entry(entity, target), target),
            _ => return,
        };
        let Some(UnitUse { slot, action }) = used else {
            return;
        };
        // The server notes each unit that comes to a container, for which a flier lands.
        if let (OrderKind::Enter { .. }, Some(_)) = (ordered.order, exits.as_ref())
            && let Some(mut riders) = index
                .get(target)
                .and_then(|container| containers.get_mut(container).ok())
            && let Ok(&id) = units.p1().get(entity).map(|(id, _)| id)
        {
            riders.board(id);
        }
        let mut users = units.p1();
        let (&id, mut unit) = users
            .get_mut(entity)
            .expect("a user holds its slots, their times and their progress");
        let dues = &mut indexes.of(id, entity);
        if unit.slots().slot(slot).and_then(|held| held.action) != Some(action) {
            unit.fill(slot, Some(action), dues);
        }
        unit.progress()
            .order(slot, ActionTarget::Unit(target), dues);
    }
}
