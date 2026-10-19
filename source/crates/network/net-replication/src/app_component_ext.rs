use crate::component_registration::ComponentRegistration;
use bevy_app::App;
use bevy_ecs::component::Component;

/// Starts a component's registration.
pub trait AppComponentExt {
    /// Starts the registration of `C`, which a replication method on it makes a networked
    /// component of the protocol:
    ///
    /// ```rust,ignore
    /// app.register_component::<MyComponent>()
    ///     .replicate()
    ///     .predict();
    /// ```
    ///
    /// A registration with no replication method, as a local rollback, leaves the protocol
    /// unchanged.
    fn register_component<C: Component>(&mut self) -> ComponentRegistration<'_, C>;
}

impl AppComponentExt for App {
    fn register_component<C: Component>(&mut self) -> ComponentRegistration<'_, C> {
        ComponentRegistration::new(self)
    }
}
