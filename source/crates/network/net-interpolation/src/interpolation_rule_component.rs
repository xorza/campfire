use crate::confirmed_history_fns::ConfirmedHistoryFns;
use bevy_ecs::component::ComponentId;
use bevy_ecs::world::World;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryComponent;

/// One component an interpolation rule holds.
///
/// The history installers live on the member instead of in a separate global provider catalog.
/// Each rule therefore carries the history behavior needed to initialize and maintain its
/// members as well as its apply behavior.
#[derive(Debug, Clone)]
pub(crate) struct InterpolationRuleComponent {
    /// Type identity used to prevent rules from claiming the same member.
    pub(crate) type_id: ComponentTypeId,
    /// Bevy component IDs whose presence can make this member available.
    pub(crate) live_component_id: ComponentId,
    pub(crate) confirmed_history_component_id: ComponentId,
    /// How its confirmed history is written, when the rule holds it.
    pub(crate) confirmed: Option<ConfirmedHistoryFns>,
}

impl InterpolationRuleComponent {
    /// The member of `C`, its ids registered in `world`, whose confirmed history `confirmed`
    /// writes when the rule holds it.
    pub(crate) fn of<C>(world: &mut World, confirmed: Option<ConfirmedHistoryFns>) -> Self
    where
        C: HistoryComponent,
    {
        Self {
            type_id: ComponentTypeId::of::<C>(),
            // Every member needs all representation IDs for archetype matching,
            // including disabled rules that own no history or apply callbacks.
            live_component_id: world.register_component::<C>(),
            confirmed_history_component_id: world.register_component::<ConfirmedHistory<C>>(),
            confirmed,
        }
    }
}
