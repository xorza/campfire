use bevy_ecs::component::{Component, Mutable};
use core::fmt::Debug;

/// A component that prediction and interpolation keep in a history: stored by `Clone`, compared by
/// `PartialEq`, and written back, as `Mutable` lets them.
pub trait HistoryComponent: Component<Mutability = Mutable> + Clone + PartialEq + Debug {}

impl<T> HistoryComponent for T where T: Component<Mutability = Mutable> + Clone + PartialEq + Debug {}
