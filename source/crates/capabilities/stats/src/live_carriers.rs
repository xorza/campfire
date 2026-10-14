use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_declare::Cache;
use campfire_sim::StableId;

/// The live stat changes of the match, each by its source and the unit that carries it, sorted by
/// source: a carrier's stats change as its source's do, with no change of its own state, so it
/// refreshes in each pass that refreshes its source. A refresh gives again the changes of each
/// unit it visits; a change whose source or carrier is gone goes. Derived, never state.
#[derive(Resource, Debug, Default)]
pub struct LiveCarriers {
    carried: Vec<Carried>,
    /// The stable ids of a pass's visits, kept between passes.
    ids: Vec<StableId>,
}

/// A live change: its source, and the unit that carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Carried {
    source: StableId,
    carrier: Entity,
}

impl LiveCarriers {
    /// Adds to `visits`, sorted and each once, the carrier of each live change whose source it
    /// visits, `id_of` giving each visit's stable id, again until no carrier joins, so a chain of
    /// live changes refreshes whole.
    pub(crate) fn join_sources(
        &mut self,
        visits: &mut Vec<Entity>,
        id_of: impl Fn(Entity) -> Option<StableId>,
    ) {
        visits.sort_unstable();
        visits.dedup();
        loop {
            let before = visits.len();
            self.ids.clear();
            self.ids
                .extend(visits.iter().filter_map(|&entity| id_of(entity)));
            for &id in &self.ids {
                let start = self.carried.partition_point(|carried| carried.source < id);
                let of_id = self.carried[start..]
                    .iter()
                    .take_while(|carried| carried.source == id);
                visits.extend(of_id.map(|carried| carried.carrier));
            }
            visits.sort_unstable();
            visits.dedup();
            if visits.len() == before {
                return;
            }
        }
    }

    /// Forgets the live changes that `visits`, sorted, carry, which their refresh gives again,
    /// and each whose source or carrier is gone, as `stands` and `carries` say.
    pub(crate) fn forget(
        &mut self,
        visits: &[Entity],
        stands: impl Fn(StableId) -> bool,
        carries: impl Fn(Entity) -> bool,
    ) {
        self.carried.retain(|carried| {
            visits.binary_search(&carried.carrier).is_err()
                && stands(carried.source)
                && carries(carried.carrier)
        });
    }

    /// Adds the live change of `source` that `carrier` carries, before `sort`.
    pub fn add(&mut self, source: StableId, carrier: Entity) {
        self.carried.push(Carried { source, carrier });
    }

    /// Sorts the changes by source, each once, after a refresh added its own.
    pub(crate) fn sort(&mut self) {
        self.carried.sort_unstable();
        self.carried.dedup();
    }
}

impl Cache for LiveCarriers {}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use campfire_sim::StableId;

    use crate::live_carriers::LiveCarriers;

    impl LiveCarriers {
        /// Each live change, by its source, then the unit that carries it.
        pub fn carried(&self) -> Vec<(StableId, Entity)> {
            let carried = self.carried.iter();
            carried
                .map(|carried| (carried.source, carried.carrier))
                .collect()
        }
    }
}
