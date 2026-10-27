use bevy::ecs::resource::Resource;
use campfire_capabilities::Directive;
use campfire_orders::OrdersDirective;
use campfire_production::{ProductionDirective, RallyTarget};
use campfire_units::GroundPoint;

/// The command whose point the player picks, from a button of the command panel, while the
/// next left click on the ground picks it (02-engine-core.md, The panel's commands are the engine's
/// orders). It exists while the input's mode is `InputMode::Aim`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AimedCommand {
    AttackMove,
    Rally,
}

impl AimedCommand {
    /// The order to the selection of its command at `to`.
    pub(super) fn directive(self, to: GroundPoint) -> Directive {
        match self {
            Self::AttackMove => OrdersDirective::AttackMove { to }.into(),
            Self::Rally => ProductionDirective::Rally {
                target: Some(RallyTarget::Point(to)),
            }
            .into(),
        }
    }
}
