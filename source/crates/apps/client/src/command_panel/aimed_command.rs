use bevy::ecs::resource::Resource;
use campfire_actions::ActionTarget;
use campfire_capabilities::{Directive, Order};
use campfire_math::Vec3;
use campfire_orders::OrdersDirective;
use campfire_production::{ProductionDirective, RallyTarget};
use campfire_sim::StableId;
use campfire_units::GroundPoint;

use crate::selection::Selection;

/// The command whose point the player picks, from a button of the command panel, while the
/// next left click on the ground picks it (02-engine-core.md, The panel's commands are the engine's
/// orders): an attack move or a rally of the selection, or the cast of a unit's slot. It exists
/// while the input's mode is `InputMode::Aim`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AimedCommand {
    AttackMove,
    Rally,
    Cast { unit: StableId, slot: u8 },
}

impl AimedCommand {
    /// The order of its command at the point `to` of the ground plane, `at` in the world: to the
    /// selection for an attack move or a rally, none for an empty one, or to its unit for a cast.
    pub(super) fn order(self, to: GroundPoint, at: Vec3, selection: &Selection) -> Option<Order> {
        let directive: Directive = match self {
            Self::AttackMove => OrdersDirective::AttackMove { to }.into(),
            Self::Rally => ProductionDirective::Rally {
                target: Some(RallyTarget::Point(to)),
            }
            .into(),
            Self::Cast { unit, slot } => {
                let target = ActionTarget::Point(at);
                return Some(Order::one(unit, OrdersDirective::Slot { slot, target }));
            }
        };
        selection.order(directive)
    }
}
