use crate::room_id::RoomId;
use bevy_ecs::prelude::*;

/// Hands out room ids, and takes back the ids of rooms that are gone, so that only the rooms
/// that live at once count toward the `u16` id space.
#[derive(Debug, Resource, Default)]
pub struct RoomAllocator {
    next_id: u16,
    /// Ids given back with [`deallocate`](Self::deallocate), handed out again first.
    free: Vec<RoomId>,
}

impl RoomAllocator {
    pub fn allocate(&mut self) -> RoomId {
        if let Some(id) = self.free.pop() {
            return id;
        }
        let id = RoomId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("at most 65,536 rooms live at once");
        id
    }

    /// Gives `room` back, once no entity or client is in it: a later [`allocate`](Self::allocate)
    /// hands the id out again.
    pub fn deallocate(&mut self, room: RoomId) {
        debug_assert!(room.0 < self.next_id, "{room:?} was never allocated");
        debug_assert!(!self.free.contains(&room), "{room:?} is deallocated twice");
        self.free.push(room);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deallocated id is handed out again before a new one.
    #[test]
    fn a_deallocated_id_is_reused() {
        let mut allocator = RoomAllocator::default();
        let [a, b] = [allocator.allocate(), allocator.allocate()];
        assert_eq!([a, b], [RoomId(0), RoomId(1)]);
        allocator.deallocate(a);
        assert_eq!(allocator.allocate(), RoomId(0));
        assert_eq!(allocator.allocate(), RoomId(2));
    }
}
