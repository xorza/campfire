use crate::component_kind::ComponentKind;
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use bevy_reflect::TypePath;

use crate::replication_kind::Replication;
use campfire_net_core::RegistrySeal;
use campfire_net_core::TypeMapper;
use campfire_net_core::{RegistryHash, RegistryHasher};

/// The replicated [`Components`](Component), and how each replicates.
///
/// ### Adding Components
///
/// You register components by composing the stack's component builder with the
/// desired Replicon rule:
///
/// ```rust,ignore
/// # use bevy_app::App;
/// # use bevy_replicon::prelude::AppRuleExt;
/// # use campfire_net_replication::AppComponentExt;
/// #
/// app.register_component::<MyComponent>().replicate();
/// ```
///
/// Each registration encodes its component through `codec`. A component that replicates through
/// Replicon's own API is unknown to prediction and interpolation.
///
/// By default, a component needs to implement `Serialize` and `Deserialize`.
///
/// Components that should only send insertions and removals can use
/// `ComponentRegistration::replicate_once`.
///
/// ```rust
/// # use bevy_app::App;
/// # use bevy_ecs::component::Component;
/// # use serde::{Deserialize, Serialize};
/// # use campfire_net_replication::AppComponentExt;
///
/// #[derive(Component, PartialEq, Serialize, Deserialize)]
/// struct MyComponent;
///
/// fn add_components(app: &mut App) {
///   app.register_component::<MyComponent>().replicate();
/// }
/// ```
///
/// ### Customizing Component behaviour
///
/// There are some cases where you might want to define additional behaviour for a component.
///
/// #### Entity Mapping
/// Replicon maps the [`Entity`] values of a component from the remote world to the local one
/// through [`Component::map_entities`], which the `Component` derive implements for each field
/// marked `#[entities]`.
///
/// #### Prediction
/// When client-prediction is enabled, a predicted entity is one that has the [`Predicted`](crate::predicted::Predicted) component.
///
/// You have to specify which components are predicted by calling `predict()`.
///
/// #### Correction
/// When client-prediction is enabled, there might be cases where there is a mismatch between the state of the Predicted entity
/// and the state of the Confirmed entity. In this case, we rollback by snapping the Predicted entity to the Confirmed entity and replaying the last few frames.
///
/// However, rollbacks that do an instant update can be visually jarring, so we provide the option
/// to smooth the rollback process over a few frames. You can do this by registering an
/// interpolation rule for the component and calling one of the `correct_*` methods on the
/// prediction registration. Correction reuses the component's registered interpolation rule to
/// sample the corrected visual state, and uses the correction function to decay the rollback error
/// over time.
///
/// #### Interpolation
/// Similarly to client-prediction, an interpolated entity has the [`Interpolated`](crate::interpolated::Interpolated) component.
///
/// Interpolated components are registered by calling `interpolate` with the interpolation
/// functions, and interpolate between two consecutive replicated updates.
/// `InterpolationFns::linear()` interpolates a component that implements `Ease`, and
/// `InterpolationFns::with(f)` takes your own function.
///
/// ```rust,ignore
/// use bevy_app::App;
/// use bevy_ecs::component::Component;
/// use serde::{Deserialize, Serialize};
/// use campfire_net_replication::AppComponentExt;
///
/// #[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// struct MyComponent(f32);
///
/// fn my_lerp_fn(start: &MyComponent, other: &MyComponent, t: f32) -> MyComponent {
///    MyComponent(start.0 * (1.0 - t) + other.0 * t)
/// }
///
/// fn add_components(app: &mut App) {
///   app.register_component::<MyComponent>().replicate()
///       .predict()
///       .interpolate(InterpolationFns::with(my_lerp_fn));
/// }
/// ```
#[derive(Debug, Default, Clone, Resource, TypePath)]
pub struct ComponentRegistry {
    pub kind_map: TypeMapper<ComponentKind>,
    seal: RegistrySeal,
    /// How each component that replicates does it, as its registration set it.
    replication: HashMap<ComponentKind, Replication>,
}

impl ComponentRegistry {
    pub fn is_registered<C: 'static>(&self) -> bool {
        self.contains(&ComponentKind::of::<C>())
    }

    pub fn contains(&self, kind: &ComponentKind) -> bool {
        self.kind_map.net_id(kind).is_some()
    }

    pub fn register_component<C: Component>(&mut self) {
        self.open();
        self.kind_map.add::<C>();
    }

    fn open(&self) {
        self.seal.assert_open("component registry");
    }

    /// Seals the registry, as the shared replication plugin's `finish` does. Its hash holds each
    /// component's type and how it replicates, in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::default();
        for (kind, name) in &self.kind_map {
            hasher.name(name);
            hasher.write(&[match self.replication(kind) {
                None => 0,
                Some(Replication::Full) => 1,
                Some(Replication::Diff) => 2,
            }]);
        }
        self.seal.seal("component registry", hasher.finish());
    }

    /// The hash peers compare, once the registry is sealed.
    pub const fn hash(&self) -> Option<RegistryHash> {
        self.seal.hash()
    }

    /// How the component replicates, if it does.
    pub fn replication(&self, kind: &ComponentKind) -> Option<Replication> {
        self.replication.get(kind).copied()
    }

    /// Records how `C` replicates.
    ///
    /// # Panics
    ///
    /// When `C` was registered to replicate the other way: one component replicates one way.
    pub(crate) fn set_replication<C: Component>(&mut self, replication: Replication) {
        self.open();
        let previous = self
            .replication
            .insert(ComponentKind::of::<C>(), replication);
        assert!(
            previous.is_none_or(|previous| previous == replication),
            "{} replicates both whole and as diffs",
            core::any::type_name::<C>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    struct Health;

    fn hash_of(build: impl FnOnce(&mut ComponentRegistry)) -> RegistryHash {
        let mut registry = ComponentRegistry::default();
        build(&mut registry);
        registry.seal();
        registry.hash().unwrap()
    }

    /// A component and how it replicates each change the hash, so the protocol check tells
    /// registries apart; the same registrations agree.
    #[test]
    fn each_component_and_its_replication_enter_the_hash() {
        let empty = hash_of(|_| {});
        let plain = hash_of(ComponentRegistry::register_component::<Health>);
        let full = hash_of(|registry| {
            registry.register_component::<Health>();
            registry.set_replication::<Health>(Replication::Full);
        });
        let diff = hash_of(|registry| {
            registry.register_component::<Health>();
            registry.set_replication::<Health>(Replication::Diff);
        });
        let hashes = [empty, plain, full, diff];
        for (i, hash) in hashes.iter().enumerate() {
            for other in &hashes[i + 1..] {
                assert_ne!(hash, other);
            }
        }
        assert_eq!(
            hash_of(ComponentRegistry::register_component::<Health>),
            plain
        );
    }

    #[test]
    #[should_panic(
        expected = "the component registry takes a registration after its plugin finished"
    )]
    fn a_sealed_registry_refuses_a_component() {
        let mut registry = ComponentRegistry::default();
        registry.seal();
        registry.register_component::<Health>();
    }
}
