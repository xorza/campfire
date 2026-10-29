use bevy_ecs::prelude::*;

use crate::room_id::RoomId;

/// Hands out room ids, and takes back the ids of rooms that are gone, so that only the rooms
/// that live at once count toward the `u16` id space.
#[derive(Debug, Resource, Default)]
pub struct RoomAllocator {
    /// The id the next new room takes, past the `u16` space once every id is handed out.
    next_id: u32,
    /// Ids given back with [`deallocate`](Self::deallocate), handed out again first.
    free: Vec<RoomId>,
}

impl RoomAllocator {
    pub fn allocate(&mut self) -> RoomId {
        if let Some(id) = self.free.pop() {
            return id;
        }
        let id = u16::try_from(self.next_id).expect("at most 65,536 rooms live at once");
        self.next_id += 1;
        RoomId(id)
    }

    /// Gives `room` back, once no entity or client is in it: a later [`allocate`](Self::allocate)
    /// hands the id out again.
    pub fn deallocate(&mut self, room: RoomId) {
        debug_assert!(
            u32::from(room.0) < self.next_id,
            "{room:?} was never allocated"
        );
        debug_assert!(!self.free.contains(&room), "{room:?} is deallocated twice");
        self.free.push(room);
    }
}

#[cfg(test)]
mod tests;
