use bevy_ecs::resource::Resource;
use campfire_sim::{SimResource, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The units each cast's projectiles hit, for projectiles that hit a unit once a cast: a cast by
/// the id of its first projectile, while one of its projectiles flies.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct CastHits(Vec<CastHit>);

/// A unit a projectile of the cast `group` hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct CastHit {
    pub(crate) group: StableId,
    pub(crate) unit: StableId,
}

impl CastHits {
    pub(crate) fn contains(&self, hit: CastHit) -> bool {
        self.0.binary_search(&hit).is_ok()
    }

    pub(crate) fn insert(&mut self, hit: CastHit) {
        if let Err(at) = self.0.binary_search(&hit) {
            self.0.insert(at, hit);
        }
    }

    /// Forgets every cast but those `flying` keeps.
    pub(crate) fn keep(&mut self, flying: impl Fn(StableId) -> bool) {
        self.0.retain(|hit| flying(hit.group));
    }
}

impl SimResource for CastHits {
    const NAME: &'static str = "projectiles.cast_hits";
}

/// A snapshot is untrusted, so hits out of order, or one twice, fail to decode: a search of them
/// relies on the order.
impl<'de> Deserialize<'de> for CastHits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CastHits, D::Error> {
        let hits = Vec::<CastHit>::deserialize(deserializer)?;
        if !hits.is_sorted_by(|a, b| a < b) {
            return Err(D::Error::custom("cast hits out of order, or one twice"));
        }
        Ok(CastHits(hits))
    }
}
