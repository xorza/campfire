use campfire_sim::Position;
use campfire_units::{ArrivedShort, Body, Destination};

use crate::gather_loop::step::Step;

/// A unit the loop goes to: where it stands, and its body.
#[derive(Debug, Clone, Copy)]
pub(super) struct Place {
    pub pos: Position,
    pub(super) body: Option<Body>,
}

impl Place {
    /// The squared distance on the ground plane from `from` to the point of the place's body
    /// nearest it, as `Shape::nearest_point` rounds it, in squared bits.
    pub(super) fn distance_from(self, from: Position) -> u128 {
        let near = Body::shape_of(self.body.as_ref()).nearest_point(self.pos, from);
        from.ground_offset(near).length_squared_bits()
    }

    /// Walks a worker at `from`, whose destination and report of a short arrival are given, to
    /// its point nearest it, or stops it when it arrived short of that point.
    pub(super) fn walk_from(
        self,
        from: Position,
        destination: Option<&Destination>,
        arrived: Option<&ArrivedShort>,
    ) -> Step {
        let to = Body::shape_of(self.body.as_ref()).nearest_point(self.pos, from);
        if Destination::gives_up(destination, arrived, to) {
            Step::Stop
        } else {
            Step::Walk(to)
        }
    }
}
