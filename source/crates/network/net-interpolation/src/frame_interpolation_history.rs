use crate::frame_interpolate::FrameInterpolate;
use crate::history_component::HistoryComponent;
use crate::interpolation_holders::InterpolationHolders;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_utils::prelude::DebugName;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::DeferredEntityCommands;
use tracing::trace;

/// Component history used by frame interpolation for component `C`.
///
/// The stack inserts and updates this component for every matching rule that
/// holds frame interpolation history. Users can also write it directly when they
/// run custom rollback or interpolation systems.
#[derive(Component, PartialEq, Eq, Debug)]
pub struct FrameInterpolationHistory<C: Component> {
    /// Value recorded at the previous fixed-update tick.
    pub previous_value: Option<C>,
    /// Value recorded at the current fixed-update tick.
    pub current_value: Option<C>,
}

impl<C: Component> Default for FrameInterpolationHistory<C> {
    fn default() -> Self {
        Self {
            previous_value: None,
            current_value: None,
        }
    }
}

impl<C: HistoryComponent> FrameInterpolationHistory<C> {
    /// Records each entity's live `C` into its frame history, where a rule that holds
    /// `C`'s frame history keeps it and no source keeps the samples instead.
    ///
    /// This runs after fixed updates so `current_value` is the latest fixed-tick value and
    /// `previous_value` is the value from the prior fixed tick. The write does not mark the history
    /// changed.
    pub(crate) fn update(
        holders: Res<'_, InterpolationHolders>,
        mut query: Query<
            '_,
            '_,
            (Entity, EntityLocation, &C, Option<&mut Self>),
            With<FrameInterpolate>,
        >,
        mut deferred_apply: Deferred<'_, DeferredEntityCommands>,
    ) {
        let type_id = ComponentTypeId::of::<C>();
        let maintained = holders.frame.history(type_id);
        let sourced = holders.frame.sourced(type_id);
        for (entity, location, component_value, history) in &mut query {
            // A source keeps the samples, so there is nothing to copy.
            if !maintained.contains(location.archetype_id)
                || sourced.contains(location.archetype_id)
            {
                continue;
            }
            let Some(mut history) = history else {
                deferred_apply.insert(
                    entity,
                    Self {
                        previous_value: None,
                        current_value: Some(component_value.clone()),
                    },
                );
                continue;
            };
            let history = history.bypass_change_detection();
            // The previous value's slot takes the new one, so a value that holds memory reuses it.
            if history.current_value.is_some() {
                core::mem::swap(&mut history.previous_value, &mut history.current_value);
            }
            match &mut history.current_value {
                Some(current_value) => current_value.clone_from(component_value),
                None => history.current_value = Some(component_value.clone()),
            }
            trace!(
                target: "campfire_net_debug::frame_interpolation",
                kind = "frame_interpolation_update_history",
                schedule = "FixedPostUpdate",
                sample_point = "FixedPostUpdate",
                component = ?DebugName::type_name::<C>(),
                "updated frame interpolation history"
            );
        }
    }
}
