use bevy::app::{App, Plugin, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, ResMut};
use campfire_net::PendingOrders;

use crate::dock_click::Docks;
use crate::right_click::{ClickSystems, RightClick};
use crate::selection::Selection;

/// A right click on a node or a drop-off orders each selected unit that gathers or returns there
/// by its own gather (02-engine-core.md, A right-click on a node gathers), and takes the click
/// when one does.
#[derive(Debug)]
pub(crate) struct GatherClickPlugin;

impl Plugin for GatherClickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::gather.in_set(ClickSystems::Gather));
    }
}

impl GatherClickPlugin {
    fn gather(
        (selection, docks): (Res<'_, Selection>, Docks<'_, '_>),
        mut click: ResMut<'_, RightClick>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(under) = click.get().and_then(|at| at.under) else {
            return;
        };
        let gathers = docks.orders(selection.get(), under.id);
        if gathers.is_empty() {
            return;
        }
        click.take();
        for order in gathers {
            orders.push(order);
        }
    }
}
