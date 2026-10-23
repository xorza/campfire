use bevy::app::{App, Plugin, Update};
use bevy::ecs::query::{Allow, Has};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res, ResMut};
use campfire_capabilities::Directive;
use campfire_net::PendingOrders;
use campfire_production::{ProductionDirective, RallyTarget, TrainQueue};
use campfire_sim::{EntityIndex, Unpredicted};
use campfire_units::MoveStep;

use crate::right_click::{ClickAt, ClickSystems, RightClick};
use crate::selection::Selection;
use crate::view::float_num;

/// A right click with producers selected and no walker sets their rally point at the unit under
/// the cursor or the ground point (02-engine-core.md, A command panel from the actions), and takes
/// the click.
#[derive(Debug)]
pub(crate) struct RallyClickPlugin;

/// Whether a unit walks, and whether it trains units.
type Movers<'w, 's> = Query<'w, 's, (Has<MoveStep>, Has<TrainQueue>), Allow<Unpredicted>>;

/// What a selection holds, as a right click reads it: producers and no walker, whose rally point
/// a click sets, or any other units, which a click moves or sends to attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selected {
    Producers,
    Movers,
}

/// A selected unit, as a right click reads it: whether it walks, and whether it trains units.
#[derive(Debug, Clone, Copy)]
struct Mover {
    walks: bool,
    trains: bool,
}

impl Plugin for RallyClickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::rally.in_set(ClickSystems::Rally));
    }
}

impl RallyClickPlugin {
    fn rally(
        selection: Res<'_, Selection>,
        (index, movers): (Res<'_, EntityIndex>, Movers<'_, '_>),
        mut click: ResMut<'_, RightClick>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(at) = click.get() else {
            return;
        };
        let held = selection.get().iter().filter_map(|&unit| {
            let (walks, trains) = movers.get(index.get(unit)?).ok()?;
            Some(Mover { walks, trains })
        });
        if Selected::of(held) != Selected::Producers {
            return;
        }
        click.take();
        let order = Self::rally_directive(at).and_then(|directive| selection.order(directive));
        if let Some(order) = order {
            orders.push(order);
        }
    }

    /// The rally a click at `at` sets: at the unit under the cursor, else at the ground point;
    /// none far off any map.
    fn rally_directive(at: ClickAt) -> Option<Directive> {
        let target = match at.under {
            Some(unit) => RallyTarget::Unit(unit.id),
            None => RallyTarget::Point(float_num::ground_point(at.point)?),
        };
        Some(
            ProductionDirective::Rally {
                target: Some(target),
            }
            .into(),
        )
    }
}

impl Selected {
    /// What a selection of `units` holds: producers when one trains and none walks.
    fn of(units: impl IntoIterator<Item = Mover>) -> Self {
        let (mut walks, mut trains) = (false, false);
        for unit in units {
            walks |= unit.walks;
            trains |= unit.trains;
        }
        if trains && !walks {
            Self::Producers
        } else {
            Self::Movers
        }
    }
}

#[cfg(test)]
mod tests;
