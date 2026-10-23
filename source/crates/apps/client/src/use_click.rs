use bevy::app::{App, Plugin, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, ResMut};
use campfire_capabilities::Order;
use campfire_interaction::Using;
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;

use crate::own_player::OwnPlayer;
use crate::right_click::{ClickSystems, RightClick};
use crate::selection::Selection;

/// A right click on a unit the own avatar may use, with the avatar selected, orders the avatar
/// alone to use it, and takes the click.
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
        (selection, using): (Res<'_, Selection>, Using<'_, '_>),
        mut click: ResMut<'_, RightClick>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(under) = click.get().and_then(|at| at.under) else {
            return;
        };
        let avatar = own
            .avatar()
            .filter(|avatar| selection.get().contains(&avatar.id));
        let Some(avatar) = avatar.filter(|avatar| using.of(avatar.entity, under.id).is_some())
        else {
            return;
        };
        click.take();
        let directive = OrdersDirective::Use { target: under.id };
        orders.push(Order::one(avatar.id, directive));
    }
}
