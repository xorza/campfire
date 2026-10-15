use bevy_ecs::component::Component;
use campfire_math::Vec3;

/// The goal a walker arrived short of, while it stands at the end of a route that ends short of
/// it: derived by `navigation` from its route as the route changes, and kept here, as
/// `Destination` is, so a capability reads it with no dependency on navigation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArrivedShort(Option<Vec3>);

impl ArrivedShort {
    /// The report of a walker that arrived short of `goal`, or of none.
    pub const fn of(goal: Option<Vec3>) -> ArrivedShort {
        ArrivedShort(goal)
    }

    /// Whether it arrived short of `goal`.
    pub fn of_goal(self, goal: Vec3) -> bool {
        self.0 == Some(goal)
    }
}
