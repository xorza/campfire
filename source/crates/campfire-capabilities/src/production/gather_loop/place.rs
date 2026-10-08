use campfire_sim::Position;

use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::production::gather_loop::step::Step;
use crate::units::body::Body;

/// A unit the loop goes to: where it stands, and its body.
#[derive(Debug, Clone, Copy)]
pub(super) struct Place {
    pub(super) at: Position,
    pub(super) body: Option<Body>,
}

impl Place {
    /// The squared distance on the ground plane from `from` to the point of the place's body
    /// nearest it, as `Shape::nearest_point` rounds it, in squared bits.
    pub(super) fn distance_from(self, from: Position) -> u128 {
        let near = Body::shape_of(self.body.as_ref()).nearest_point(self.at, from);
        from.ground_offset(near).length_squared_bits()
    }

    /// Walks a worker at `from`, whose route and destination are given, to its point
    /// nearest it, or stops it when its route arrived short of that point.
    pub(super) fn walk_from(
        self,
        from: Position,
        destination: Option<&Destination>,
        route: Option<&Route>,
    ) -> Step {
        let to = Body::shape_of(self.body.as_ref()).nearest_point(self.at, from);
        if Destination::gives_up(destination, route, to) {
            Step::Stop
        } else {
            Step::Walk(to)
        }
    }
}
