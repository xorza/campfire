use bevy_ecs::change_detection::Tick as ChangeTick;
use bevy_ecs::resource::Resource;

use crate::entity_index::EntityIndex;
use crate::stable_id::StableId;

/// What a match world records for the copies of its state, from `StateRegistry::track` on: the
/// world's change tick at the last copy, the stable ids it gained and lost, and the ids whose
/// component of a registered type was removed, by the type's place in the registry. A value
/// counts as changed when its change tick is newer than the last copy's.
#[derive(Resource, Debug)]
pub(crate) struct StateChanges {
    /// None before the first copy, which takes every value.
    since: Option<ChangeTick>,
    gained: Vec<StableId>,
    lost: Vec<StableId>,
    removed: Vec<Removal>,
}

/// A component of the registered type at `entry` removed from the entity of `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Removal {
    pub(crate) entry: u16,
    pub(crate) id: StableId,
}

impl StateChanges {
    /// Changes for a first copy of every value, of the entities `index` holds.
    pub(crate) fn new(index: &EntityIndex) -> StateChanges {
        StateChanges {
            since: None,
            gained: index.iter().map(|(id, _)| id).collect(),
            lost: Vec::new(),
            removed: Vec::new(),
        }
    }

    pub(crate) fn gain(&mut self, id: StableId) {
        self.gained.push(id);
    }

    pub(crate) fn lose(&mut self, id: StableId) {
        self.lost.push(id);
    }

    pub(crate) fn remove(&mut self, removal: Removal) {
        self.removed.push(removal);
    }

    pub(crate) const fn since(&self) -> Option<ChangeTick> {
        self.since
    }

    /// Sorts what it recorded, each once: the ids lost that the last copy held, as an id gained
    /// since never reached it, and the ids gained that `index` still holds.
    pub(crate) fn settle(&mut self, index: &EntityIndex) {
        self.gained.sort_unstable();
        self.gained.dedup();
        self.lost.sort_unstable();
        self.lost.dedup();
        let gained = &self.gained;
        self.lost.retain(|id| gained.binary_search(id).is_err());
        self.gained.retain(|&id| index.get(id).is_some());
        self.removed.sort_unstable();
        self.removed.dedup();
    }

    /// The ids gained, once settled.
    pub(crate) fn gained(&self) -> &[StableId] {
        &self.gained
    }

    /// The ids lost, once settled.
    pub(crate) fn lost(&self) -> &[StableId] {
        &self.lost
    }

    /// The removals of the type at `entry`, once settled.
    pub(crate) fn removed(&self, entry: u16) -> &[Removal] {
        let start = self
            .removed
            .partition_point(|removal| removal.entry < entry);
        let end = self
            .removed
            .partition_point(|removal| removal.entry <= entry);
        &self.removed[start..end]
    }

    /// Starts recording again, for the copy after the one made at `now`.
    pub(crate) fn restart(&mut self, now: ChangeTick) {
        self.since = Some(now);
        self.gained.clear();
        self.lost.clear();
        self.removed.clear();
    }
}
