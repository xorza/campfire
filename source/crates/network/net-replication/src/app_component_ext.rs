use bevy_app::App;
use bevy_ecs::component::Component;

use crate::component_registration::ComponentRegistration;

/// Starts a component's registration.
pub trait AppComponentExt {
    /// Starts the registration of `C`, which a replication method on it makes a networked
    /// component of the protocol; one with no replication method, as a local rollback, leaves the
    /// protocol unchanged.
    fn register_component<C: Component>(&mut self) -> ComponentRegistration<'_, C>;
}

impl AppComponentExt for App {
    fn register_component<C: Component>(&mut self) -> ComponentRegistration<'_, C> {
        ComponentRegistration::new(self)
    }
}
