use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::{SimResource, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The units each line projectile struck, so it strikes a unit once: by the projectile, or by
/// its cast, named by its first projectile, for a type that strikes a unit once a cast; each kept
/// while one that strikes by it flies. One sorted run, so a test of a hit costs a search.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StruckUnits(Vec<Struck>);

/// A unit a projectile struck, by the projectile or its cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Struck {
    pub by: StableId,
    pub id: StableId,
}

impl StruckUnits {
    pub fn contains(&self, struck: Struck) -> bool {
        self.0.binary_search(&struck).is_ok()
    }

    pub(crate) fn insert(&mut self, struck: Struck) {
        if let Err(at) = self.0.binary_search(&struck) {
            self.0.insert(at, struck);
        }
    }

    /// Forgets the hits of every projectile and cast but those in `flying`, sorted, in one pass
    /// over both.
    pub fn keep(&mut self, flying: &[StableId]) {
        debug_assert!(flying.is_sorted(), "the flying are sorted");
        let mut next = 0;
        self.0.retain(|struck| {
            next += flying[next..].partition_point(|&by| by < struck.by);
            flying.get(next) == Some(&struck.by)
        });
    }
}

impl SimResource for StruckUnits {
    const NAME: &'static str = "projectiles.struck_units";

    // Each id may be gone, which every reader allows.
    fn check(&self, _: &World) -> bool {
        true
    }
}

/// A snapshot is untrusted, so hits out of order, or one twice, fail to decode: a search of them
/// relies on the order.
impl<'de> Deserialize<'de> for StruckUnits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hits = Vec::<Struck>::deserialize(deserializer)?;
        if !hits.is_sorted_by(|a, b| a < b) {
            return Err(D::Error::custom("struck units out of order, or one twice"));
        }
        Ok(Self(hits))
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn struck_units_keep_only_the_flying() {
        let mut ids = IdAllocator::default();
        let [a, b, c, id] = [(); 4].map(|()| ids.allocate());
        let mut struck = StruckUnits::default();
        for by in [c, a, b, a] {
            struck.insert(Struck { by, id });
        }
        assert!(struck.contains(Struck { by: a, id }));
        assert!(!struck.contains(Struck { by: id, id }));
        // Of a, b and c, each struck once, a and c fly on.
        struck.keep(&[a, c, id]);
        let kept: Vec<_> = struck.0.iter().map(|struck| struck.by).collect();
        assert_eq!(kept, [a, c]);
        struck.keep(&[]);
        assert_eq!(struck, StruckUnits::default());
    }
}
