use bevy_reflect::Reflect;

/// How long a `ControlledBy` entity lives on the sender.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ControlledLifetime {
    #[default]
    /// When the client that controls the entity disconnects, the entity is despawned on the sender.
    Link,
    /// The entity's life does not depend on the controlling client: it stays on the sender when
    /// the client disconnects.
    Independent,
}
