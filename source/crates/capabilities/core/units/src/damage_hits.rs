use std::slice;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Report, Role};
use campfire_sim::StableId;

/// The hits the tick's damage dealt, each its target and its source, in the order dealt: what a
/// capability that answers a hit reads once the damage is dealt, as a damaged unit calls its
/// allies (combat-response.md, A damaged unit calls its idle allies). Not state: it empties before
/// each tick's damage, and a match that runs no damage holds none.
#[derive(Resource, Debug, Default)]
pub struct DamageHits(Vec<DamageHit>);

/// A hit the damage dealt: the unit it struck, dead since or not, and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageHit {
    pub target: StableId,
    pub source: StableId,
}

impl DamageHits {
    /// Empties it for the next tick's damage.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Records `hit`, as the damage deals it.
    pub fn push(&mut self, hit: DamageHit) {
        self.0.push(hit);
    }

    pub fn iter(&self) -> slice::Iter<'_, DamageHit> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a DamageHits {
    type Item = &'a DamageHit;
    type IntoIter = slice::Iter<'a, DamageHit>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A match that computes the state deals the damage, so a client holds none.
impl Report for DamageHits {
    fn installed(role: &Role) -> Option<Self> {
        role.computes_state().then(Self::default)
    }
}
