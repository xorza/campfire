use crate::frame_interpolate::FrameInterpolate;
use crate::history_component::HistoryComponent;
use crate::interpolation_owners::InterpolationOwners;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_utils::prelude::DebugName;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::DeferredEntityCommands;
use tracing::trace;

/// Component history used by frame interpolation for component `C`.
///
/// The stack inserts and updates this component for every matching rule that
/// owns frame interpolation history. Users can also write it directly when they
/// run custom rollback or interpolation systems.
///
/// # Examples
///
/// ```rust,ignore
/// use bevy_ecs::prelude::*;
/// use crate::frame_interpolation_history::FrameInterpolationHistory;
///
/// #[derive(Component, Clone, PartialEq)]
/// struct Position(f32);
///
/// fn seed(mut commands: Commands, entity: Entity) {
///     commands.entity(entity).insert(FrameInterpolationHistory::<Position> {
///         previous_value: Some(Position(0.0)),
///         current_value: Some(Position(1.0)),
///     });
/// }
/// ```
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
    /// Records each entity's live `C` into its frame history, where a rule that owns
    /// `C`'s frame history keeps it and no source keeps the samples instead.
    ///
    /// This runs after fixed updates so `current_value` is the latest fixed-tick
    /// value and `previous_value` is the value from the prior fixed tick. The write does not mark the
    /// history changed.
    pub(crate) fn update(
        owners: Res<'_, InterpolationOwners>,
        mut query: Query<
            '_,
            '_,
            (Entity, EntityLocation, &C, Option<&mut Self>),
            With<FrameInterpolate>,
        >,
        mut deferred_apply: Deferred<'_, DeferredEntityCommands>,
    ) {
        let kind = ComponentKind::of::<C>();
        let maintained = owners.frame.history(kind);
        let sourced = owners.frame.sourced(kind);
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
            // The previous value's slot takes the new one, so a value that owns memory reuses it.
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
