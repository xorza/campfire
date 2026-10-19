use crate::mismatch_fn::MismatchFn;
use bevy_app::App;
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use campfire_net_replication::ComponentKind;
use core::any::Any;
use std::sync::Arc;

/// What the registry keeps of one component that rolls back: its history, and its prediction
/// when a network replicates it.
#[derive(Debug, Clone)]
pub(crate) struct ComponentFns {
    /// Id of the [`PredictionHistory<C>`](crate::prediction_history::PredictionHistory)
    /// component.
    pub(crate) prediction_history_id: ComponentId,
    /// `None` for a component that rolls back locally, with no confirmed value to check.
    pub(crate) prediction: Option<PredictionFns>,
}

/// What the registry keeps of a networked predicted component.
#[derive(Debug, Clone)]
pub(crate) struct PredictionFns {
    /// Whether a correction smooths the component's jumps, the stack's own or one the app runs,
    /// so a rollback saves the value it replaces.
    pub(crate) corrected: bool,
    /// The [`MismatchFn<C>`] that compares the confirmed value with the predicted one: whether
    /// they differ, which asks for a rollback. `PartialEq::ne` unless the app sets its own.
    mismatch: Arc<dyn Any + Send + Sync>,
}

impl PredictionFns {
    fn new<C: PartialEq + 'static>() -> Self {
        let mismatch: MismatchFn<C> = <C as PartialEq>::ne;
        Self {
            corrected: false,
            mismatch: Arc::new(mismatch),
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct PredictionRegistry {
    /// The components that roll back, by kind.
    components: Vec<(ComponentKind, ComponentFns)>,
    /// What each registration adds to a predicting app once every plugin is in, as its receive
    /// fns need the markers' plugins: run by [`install`].
    installs: Vec<fn(&mut App)>,
    /// Set by [`seal`], after which a registration would install no system.
    finalized: bool,
}

/// The app's prediction registry, created by its first registration, which must come before the
/// app finishes its plugins.
pub(crate) fn open(app: &mut App) -> Mut<'_, PredictionRegistry> {
    let registry = app.world_mut().get_resource_or_init::<PredictionRegistry>();
    assert!(
        !registry.finalized,
        "a component registers for prediction before the app finishes its plugins"
    );
    registry
}

/// Refuses each later registration, on a server as on a client:
/// `PredictionRegistryPlugin::finish`.
pub(crate) fn seal(app: &mut App) {
    open(app).finalized = true;
}

/// Runs each registration's install, which only a predicting app needs:
/// `PredictionPlugin::finish`. The protocol's registrations are complete by then, whether or
/// not [`seal`] ran first.
pub(crate) fn install(app: &mut App) {
    let installs = core::mem::take(
        &mut app
            .world_mut()
            .resource_mut::<PredictionRegistry>()
            .installs,
    );
    for install in installs {
        install(app);
    }
}

impl PredictionRegistry {
    fn fns(&self, kind: ComponentKind) -> Option<&ComponentFns> {
        let at = self
            .components
            .binary_search_by_key(&kind, |&(kind, _)| kind)
            .ok()?;
        Some(&self.components[at].1)
    }

    fn fns_mut(&mut self, kind: ComponentKind) -> Option<&mut ComponentFns> {
        let at = self
            .components
            .binary_search_by_key(&kind, |&(kind, _)| kind)
            .ok()?;
        Some(&mut self.components[at].1)
    }

    /// Queues `install` for [`finish`].
    pub(crate) fn defer(&mut self, install: fn(&mut App)) {
        self.installs.push(install);
    }

    /// Adds the prediction of `C`, whose rollback [`Self::register_rollback`] registered.
    pub(crate) fn register<C: PartialEq + 'static>(&mut self) {
        self.fns_mut(ComponentKind::of::<C>())
            .expect("rollback metadata should be registered before prediction metadata")
            .prediction
            .get_or_insert_with(PredictionFns::new::<C>);
    }

    /// Adds the rollback of `C`, whose prediction history is the component of this id.
    pub(crate) fn register_rollback<C: 'static>(&mut self, prediction_history_id: ComponentId) {
        let kind = ComponentKind::of::<C>();
        if let Err(at) = self
            .components
            .binary_search_by_key(&kind, |&(kind, _)| kind)
        {
            let fns = ComponentFns {
                prediction_history_id,
                prediction: None,
            };
            self.components.insert(at, (kind, fns));
        }
    }

    /// The ids of the [`PredictionHistory`](crate::prediction_history::PredictionHistory)
    /// components of the networked predicted components.
    pub(crate) fn prediction_history_ids(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .map(|(_, fns)| fns)
            .filter(|fns| fns.prediction.is_some())
            .map(|fns| fns.prediction_history_id)
    }

    fn prediction<C: 'static>(&self) -> Option<&PredictionFns> {
        self.fns(ComponentKind::of::<C>())?.prediction.as_ref()
    }

    /// # Panics
    ///
    /// When `C` is not registered for prediction.
    fn prediction_mut<C: 'static>(&mut self) -> &mut PredictionFns {
        self.fns_mut(ComponentKind::of::<C>())
            .and_then(|fns| fns.prediction.as_mut())
            .expect(
                "The component has not been registered for prediction. Did you call `.predict()`?",
            )
    }

    pub(crate) fn set_mismatch<C: 'static>(&mut self, mismatch: MismatchFn<C>) {
        self.prediction_mut::<C>().mismatch = Arc::new(mismatch);
    }

    /// The compare fn of `C`, a predicted component.
    pub(crate) fn mismatch<C: 'static>(&self) -> MismatchFn<C> {
        *self
            .prediction::<C>()
            .expect("the component is predicted")
            .mismatch
            .downcast_ref()
            .expect("a component's compare fn is its own type's")
    }

    /// Whether a correction smooths `C`'s jumps.
    pub(crate) fn corrects<C: 'static>(&self) -> bool {
        self.prediction::<C>()
            .is_some_and(|prediction| prediction.corrected)
    }

    /// Marks `C` corrected, by the stack's correction or one the app runs.
    ///
    /// # Panics
    ///
    /// When `C` already registered a correction: a component takes one.
    pub(crate) fn set_corrected<C: 'static>(&mut self) {
        let prediction = self.prediction_mut::<C>();
        assert!(
            !prediction.corrected,
            "{} registers more than one correction",
            core::any::type_name::<C>()
        );
        prediction.corrected = true;
    }
}

#[cfg(test)]
mod tests;
