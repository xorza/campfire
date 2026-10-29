use bevy_ecs::component::Mutable;
use bevy_ecs::prelude::*;
use campfire_net_replication::ComponentRegistration;
use campfire_net_replication::ComponentRegistrator;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::NotReplicated;

use crate::predicted_component_registration::PredictedComponentRegistration;
use crate::prediction_mode::PredictionMode;
use crate::prediction_registration;
use crate::prediction_registry;

/// Prediction on a component's registration, which takes its mode from the registration's
/// replication kind.
pub trait PredictionRegistrationExt<'a, C, M>: ComponentRegistrator<'a, C, M> {
    /// Enable prediction, whole or from diffs as the component replicates, and return a state
    /// that exposes prediction-only configuration methods.
    fn predict(self) -> PredictedComponentRegistration<'a, C, M>
    where
        C: HistoryComponent,
        M: PredictionMode<C>;

    /// Enable rollback for a component or resource that no network replicates: it rolls back
    /// with the predicted entities.
    fn local_rollback(self) -> ComponentRegistration<'a, C, M>
    where
        C: Component<Mutability = Mutable> + Clone,
        M: LocalRollbackMode;
}

/// The replication kind a local rollback takes: none.
pub trait LocalRollbackMode {}

impl LocalRollbackMode for NotReplicated {}

impl<'a, C, M, R> PredictionRegistrationExt<'a, C, M> for R
where
    R: ComponentRegistrator<'a, C, M>,
{
    fn predict(self) -> PredictedComponentRegistration<'a, C, M>
    where
        C: HistoryComponent,
        M: PredictionMode<C>,
    {
        let registration = self.into_component_registration();
        let app = registration.app;
        prediction_registration::register_prediction_fns::<C>(app);
        prediction_registry::open(app).defer(M::install);
        PredictedComponentRegistration::new(ComponentRegistration::new(app))
    }

    fn local_rollback(self) -> ComponentRegistration<'a, C, M>
    where
        C: Component<Mutability = Mutable> + Clone,
        M: LocalRollbackMode,
    {
        let registration = self.into_component_registration();
        prediction_registration::add_local_rollback::<C>(registration.app);
        registration
    }
}
