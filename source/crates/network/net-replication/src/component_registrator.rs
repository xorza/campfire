use crate::component_registration::ComponentRegistration;

/// A builder state that can be converted to and from the base component
/// registration builder, of the same replication kind `M`.
///
/// Extension traits use this to compose registration domains without requiring
/// callers to manually unwrap typed builder states.
pub trait ComponentRegistrator<'a, C, M>: Sized {
    fn into_component_registration(self) -> ComponentRegistration<'a, C, M>;
    fn from_component_registration(registration: ComponentRegistration<'a, C, M>) -> Self;
}

impl<'a, C, M> ComponentRegistrator<'a, C, M> for ComponentRegistration<'a, C, M> {
    fn into_component_registration(self) -> Self {
        self
    }

    fn from_component_registration(registration: Self) -> Self {
        registration
    }
}
