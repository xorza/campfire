use bevy::app::{App, Plugin, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, ResMut};
use campfire_capabilities::Order;
use campfire_interaction::Using;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_sim::{EntityIndex, StableId};

use crate::own_player::OwnPlayer;
use crate::right_click::{ClickSystems, RightClick};
use crate::selection::Selection;

/// A right click on a unit the own avatar may use, with the avatar selected, orders the avatar
/// alone to use it; else a right click on a container orders each selected unit that may enter
/// it in (transports.md, The client orders entries by the right click); either takes the click.
#[derive(Debug)]
pub(crate) struct UseClickPlugin;

impl Plugin for UseClickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::use_unit.in_set(ClickSystems::Use));
    }
}

impl UseClickPlugin {
    fn use_unit(
        own: OwnPlayer<'_, '_>,
        (selection, index, using): (Res<'_, Selection>, Res<'_, EntityIndex>, Using<'_, '_>),
        mut click: ResMut<'_, RightClick>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(under) = click.get().and_then(|at| at.under) else {
            return;
        };
        let avatar = own
            .avatar()
            .filter(|avatar| selection.get().contains(&avatar.id));
        if let Some(avatar) = avatar.filter(|avatar| using.of(avatar.entity, under.id).is_some()) {
            click.take();
            let directive = OrdersDirective::Use { target: under.id };
            orders.push(Order::one(avatar.id, directive));
            return;
        }
        let enters = |unit| {
            let entity = index.get(unit);
            entity.is_some_and(|entity| using.entry(entity, under.id).is_some())
        };
        let entries = Self::entries(selection.get(), under.id, enters);
        if entries.is_empty() {
            return;
        }
        click.take();
        for entry in entries {
            orders.push(entry);
        }
    }

    /// The entries of `container` a right click orders of `selection`: each selected unit that
    /// `enters` says may enter it, ordered in alone, in the selection's order.
    fn entries(
        selection: &[StableId],
        container: StableId,
        enters: impl Fn(StableId) -> bool,
    ) -> Vec<Order> {
        let entering = selection.iter().copied().filter(|&unit| enters(unit));
        let enter = OrdersDirective::Enter { target: container };
        entering.map(|unit| Order::one(unit, enter)).collect()
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_right_click_on_a_container_orders_each_selected_unit_that_may_enter_it_in() {
        // Of a Ranger, a tank and a second Ranger selected, the Rangers alone may enter the
        // Humvee: each is ordered in alone, in the selection's order; of none that may, none.
        let mut ids = IdAllocator::default();
        let [ranger, tank, second, humvee] = [(); 4].map(|()| ids.allocate());
        let selection = [ranger, tank, second];
        let enter = OrdersDirective::Enter { target: humvee };
        assert_eq!(
            UseClickPlugin::entries(&selection, humvee, |unit| unit != tank),
            [Order::one(ranger, enter), Order::one(second, enter)]
        );
        assert_eq!(UseClickPlugin::entries(&selection, humvee, |_| false), []);
    }
}
