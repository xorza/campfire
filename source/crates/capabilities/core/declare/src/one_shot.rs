use bevy_ecs::resource::Resource;
use bevy_ecs::system::{SystemId, SystemInput};

/// The resource that keeps the id of a system its module runs out of the schedule, Bevy's
/// one-shot system, which the install registers: its declaration is `Declarations::one_shot`.
pub trait OneShot: Resource {
    /// What the system takes as it runs.
    type In: SystemInput + 'static;
    /// What the system gives back.
    type Out: 'static;

    fn new(id: SystemId<Self::In, Self::Out>) -> Self;
}
