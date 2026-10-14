use bevy_ecs::resource::Resource;
use campfire_declare::Scratch;
use campfire_sim::Position;

use crate::body_index::{BodyIndex, IndexedBody};
use crate::segment::Segment;
use crate::walker::Walker;

/// What changed in the static bodies since walkers last checked their routes: the bodies put in,
/// and whether any was taken away. A route clear of the static bodies stays clear until one is
/// put in, so a check tests it against those alone. Derived from the static index's updates, not
/// state; the check empties it, and keeps its buffer.
#[derive(Resource, Debug, Default)]
pub struct StaticChanges {
    added: Vec<IndexedBody>,
    removed: bool,
}

impl StaticChanges {
    /// Notes the last update of `statics`.
    pub(crate) fn note(&mut self, statics: &BodyIndex) {
        self.added.extend_from_slice(statics.added());
        self.removed |= !statics.removed().is_empty();
    }

    pub(crate) const fn removed(&self) -> bool {
        self.removed
    }

    /// Whether a body put in blocks `walker` on its way from `from` along `waypoints`: one of its
    /// layer that comes closer to a leg than the walker's radius.
    pub(crate) fn blocks_route(
        &self,
        from: Position,
        waypoints: &[Position],
        walker: Walker,
    ) -> bool {
        let mut at = from;
        let ours = || self.added.iter().filter(|body| body.layer == walker.layer);
        waypoints.iter().any(|&next| {
            let leg = Segment::new(at, next);
            at = next;
            ours().any(|body| body.comes_within(leg, walker.radius))
        })
    }

    /// Forgets the changes, as every walker checked its route.
    pub fn clear(&mut self) {
        self.added.clear();
        self.removed = false;
    }
}

impl Scratch for StaticChanges {
    fn is_clear(&self) -> bool {
        self.added.is_empty() && !self.removed
    }
}

#[cfg(test)]
mod tests;
