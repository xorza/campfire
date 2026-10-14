use bevy_ecs::component::Component;
use campfire_sim::Position;

/// The goal a walker arrived short of, while it stands at the end of a route that ends short of
/// it: derived by `navigation` from its route as the route changes, and kept here, as
/// `Destination` is, so a capability reads it with no dependency on navigation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArrivedShort(Option<Position>);

impl ArrivedShort {
    /// The report of a walker that arrived short of `goal`, or of none.
    pub const fn of(goal: Option<Position>) -> ArrivedShort {
        ArrivedShort(goal)
    }

    /// Whether it arrived short of `goal`.
    pub fn of_goal(self, goal: Position) -> bool {
        self.0 == Some(goal)
    }
}
