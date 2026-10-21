use crate::replication_mode::ReplicationMode;
use crate::replication_target::InterpolatedSend;
use crate::replication_target::InterpolationTarget;
use crate::replication_target::PredictionTarget;
use crate::targets_overlap::TargetsOverlap;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::{ScopeLifetime, SingleComponent, VisibilityFilter};
use campfire_log::LogEvent;
use campfire_net_connection::{NetworkTarget, RemoteId};

/// Hides [`InterpolatedSend`] on each link the entity's [`PredictionTarget`] names, so a client
/// that both targets name gets `Predicted` alone: Replicon shows a component where every filter of
/// its scope does (11-net-stack/replication.md, A client that both targets name is predicted
/// alone).
///
/// It mirrors the prediction target's mode, kept in step by [`Self::follow_prediction`] and
/// [`Self::leave_with_prediction`]. The prediction target requires it, hiding nothing, so an
/// entity spawns with it and a mirror replaces it in its archetype.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[component(immutable)]
pub(crate) struct InterpolationYield(ReplicationMode);

impl InterpolationYield {
    /// The mirror of a prediction target that names no link, as the target's spawn requires it.
    pub(crate) const fn hiding_nothing() -> Self {
        Self(ReplicationMode::Target(NetworkTarget::None))
    }

    /// A prediction target, inserted or replaced, gives the entity its mirror, and a link both
    /// targets name logs a warning.
    pub(crate) fn follow_prediction(
        trigger: On<'_, '_, Insert<PredictionTarget>>,
        targets: Query<'_, '_, (&PredictionTarget, Option<&InterpolationTarget>)>,
        links: Query<'_, '_, (Entity, &RemoteId)>,
        mut commands: Commands<'_, '_>,
    ) {
        let entity = trigger.entity;
        let Ok((prediction, interpolation)) = targets.get(entity) else {
            return;
        };
        if let Some(interpolation) = interpolation {
            Self::warn_overlap(entity, prediction, interpolation, &links);
        }
        commands
            .entity(entity)
            .try_insert(Self(prediction.mode().clone()));
    }

    /// An interpolation target, inserted or replaced, logs a warning for a link the prediction
    /// target names too.
    pub(crate) fn warn_on_interpolation(
        trigger: On<'_, '_, Insert<InterpolationTarget>>,
        targets: Query<'_, '_, (&PredictionTarget, &InterpolationTarget)>,
        links: Query<'_, '_, (Entity, &RemoteId)>,
    ) {
        if let Ok((prediction, interpolation)) = targets.get(trigger.entity) {
            Self::warn_overlap(trigger.entity, prediction, interpolation, &links);
        }
    }

    /// The mirror goes with the prediction target, unless the entity is despawned.
    pub(crate) fn leave_with_prediction(
        trigger: On<'_, '_, Remove<PredictionTarget>>,
        mut commands: Commands<'_, '_>,
    ) {
        if trigger.trigger().new_archetype.is_none() {
            return;
        }
        commands.entity(trigger.entity).try_remove::<Self>();
    }

    /// Logs each connected link that both targets name, which gets `Predicted` alone.
    fn warn_overlap(
        entity: Entity,
        prediction: &PredictionTarget,
        interpolation: &InterpolationTarget,
        links: &Query<'_, '_, (Entity, &RemoteId)>,
    ) {
        for (link, remote) in links {
            if prediction.is_visible_for(link, Some(remote))
                && interpolation.is_visible_for(link, Some(remote))
            {
                let overlap = TargetsOverlap {
                    entity: entity.to_bits(),
                    link: link.to_bits(),
                };
                overlap.log();
            }
        }
    }
}

impl VisibilityFilter for InterpolationYield {
    type ClientComponent = RemoteId;
    type Scope = SingleComponent<InterpolatedSend>;
    const LIFETIME: ScopeLifetime = ScopeLifetime::WhileVisible;

    fn is_visible(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        !self.0.is_visible_for(link, remote)
    }
}
