use bevy_ecs::prelude::*;
use bevy_replicon::prelude::VisibilityFilter;
use smallvec::SmallVec;

use crate::room_id::RoomId;

/// The rooms of an entity or a link, sorted, each once, inline up to eight: an entity replicates to
/// each link that shares a room with it, by ids from the
/// [`RoomAllocator`](crate::room_allocator::RoomAllocator).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Component)]
#[component(immutable)]
pub struct Rooms {
    rooms: SmallVec<[RoomId; 8]>,
}

impl FromIterator<RoomId> for Rooms {
    fn from_iter<T: IntoIterator<Item = RoomId>>(rooms: T) -> Self {
        let mut rooms: SmallVec<[RoomId; 8]> = rooms.into_iter().collect();
        rooms.sort_unstable();
        rooms.dedup();
        Self { rooms }
    }
}

impl<const N: usize> From<[RoomId; N]> for Rooms {
    fn from(rooms: [RoomId; N]) -> Self {
        rooms.into_iter().collect()
    }
}

impl Rooms {
    pub fn single(room: RoomId) -> Self {
        Self {
            rooms: SmallVec::from_slice(&[room]),
        }
    }

    /// The rooms, in order.
    pub fn rooms(&self) -> impl ExactSizeIterator<Item = RoomId> + '_ {
        self.rooms.iter().copied()
    }

    /// Adds an extra room to the list of rooms
    pub fn add_room(&mut self, room: RoomId) {
        if let Err(index) = self.rooms.binary_search(&room) {
            self.rooms.insert(index, room);
        }
    }

    /// Returns true if this entity/client is in the specified room
    pub fn contains_room(&self, room: RoomId) -> bool {
        self.rooms.binary_search(&room).is_ok()
    }

    /// Removes the entity/client from the specified room
    pub fn remove_room(&mut self, room: RoomId) {
        if let Ok(index) = self.rooms.binary_search(&room) {
            self.rooms.remove(index);
        }
    }

    /// Whether the two sets share a room: a merge of the two sorted lists.
    fn shares_a_room(&self, other: &Self) -> bool {
        let (mut ours, mut theirs) = (self.rooms.iter().peekable(), other.rooms.iter().peekable());
        while let (Some(our), Some(their)) = (ours.peek(), theirs.peek()) {
            match our.cmp(their) {
                core::cmp::Ordering::Less => {
                    ours.next();
                }
                core::cmp::Ordering::Greater => {
                    theirs.next();
                }
                core::cmp::Ordering::Equal => return true,
            }
        }
        false
    }
}

impl VisibilityFilter for Rooms {
    type ClientComponent = Self;
    type Scope = Entity;
    fn is_visible(&self, _client: Entity, component: Option<&Self::ClientComponent>) -> bool {
        component.is_some_and(|other| self.shares_a_room(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets of the same rooms are equal however they were built: one that held room 9 and lost
    /// it again equals one that never did, and the order and repeats of a list do not count.
    #[test]
    fn a_room_set_equals_another_of_its_rooms_however_built() {
        let mut grown = Rooms::from([RoomId(1), RoomId(9)]);
        grown.remove_room(RoomId(9));
        assert_eq!(grown, Rooms::single(RoomId(1)));
        assert_ne!(grown, Rooms::single(RoomId(2)));
        assert_eq!(
            Rooms::from([RoomId(4), RoomId(2), RoomId(4)]),
            Rooms::from([RoomId(2), RoomId(4)])
        );
        grown.add_room(RoomId(0));
        grown.add_room(RoomId(1));
        assert_eq!(grown.rooms().collect::<Vec<_>>(), [RoomId(0), RoomId(1)]);
        assert!(grown.contains_room(RoomId(0)) && !grown.contains_room(RoomId(9)));
    }

    /// Two sets are visible to each other when they share a room, wherever it sits in each list,
    /// and an empty set shares none.
    #[test]
    fn sets_that_share_a_room_see_each_other() {
        let rooms = |ids: &[u16]| ids.iter().map(|&id| RoomId(id)).collect::<Rooms>();
        let cases = [
            (rooms(&[1, 5, 9]), rooms(&[2, 9]), true),
            (rooms(&[1, 5, 9]), rooms(&[0, 1]), true),
            (rooms(&[1, 5, 9]), rooms(&[2, 6, 10]), false),
            (rooms(&[]), rooms(&[1]), false),
        ];
        for (ours, theirs, shared) in cases {
            assert_eq!(ours.shares_a_room(&theirs), shared, "{ours:?} {theirs:?}");
            assert_eq!(theirs.shares_a_room(&ours), shared, "{ours:?} {theirs:?}");
        }
        assert!(!rooms(&[1]).is_visible(Entity::PLACEHOLDER, None));
    }
}
