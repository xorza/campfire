use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

/// The kind of entity a state type sits on, which a restore checks before the value's own rules:
/// a value on another kind of entity fails it, so no system meets one where it does not belong.
pub trait HostRule: Debug + Send + Sync + 'static {
    /// Whether `entity` is of the kind, in `world`, with every state type restored.
    fn holds(world: &World, entity: Entity) -> bool;
}

/// Any entity: a type of `sim`, which knows no kinds, or of a test.
#[derive(Debug)]
pub struct OnAny;

impl HostRule for OnAny {
    fn holds(_: &World, _: Entity) -> bool {
        true
    }
}

/// An entity of kind `A` or of kind `B`.
#[derive(Debug)]
pub struct Either<A: HostRule, B: HostRule>(PhantomData<(A, B)>);

impl<A: HostRule, B: HostRule> HostRule for Either<A, B> {
    fn holds(world: &World, entity: Entity) -> bool {
        A::holds(world, entity) || B::holds(world, entity)
    }
}
