use std::collections::BTreeMap;

use crate::channel_message::ChannelMessage;
use crate::message_id::MessageId;
use crate::receive_budget::{ReceiveBudget, Reservation};

/// What a reliable channel holds past a gap after the id it waits for, by id: an entry and the
/// bytes it owns are charged to the receive budget, its share of the store's nodes included, and
/// given back as it goes (11-net-stack/transport.md, Each link's `Channels` has one receive
/// budget). A B-tree, so each insert and each take costs `O(log n)` in whatever order a peer sends
/// its ids, where a sorted list moves half its entries for each one in its middle.
#[derive(Debug)]
pub(crate) struct GapStore<T> {
    /// Each entry by its id's bits: the store finds an id, and orders nothing by it.
    entries: BTreeMap<u16, T>,
    /// The bytes the entries own past their slots.
    owned: usize,
}

/// An entry of a [`GapStore`].
pub(crate) trait GapEntry {
    fn id(&self) -> MessageId;

    /// The bytes the entry owns past its slot.
    fn owned_len(&self) -> usize;
}

impl GapEntry for MessageId {
    fn id(&self) -> MessageId {
        *self
    }

    fn owned_len(&self) -> usize {
        0
    }
}

impl GapEntry for ChannelMessage {
    fn id(&self) -> MessageId {
        self.message_id.expect("a reliable message has an id")
    }

    fn owned_len(&self) -> usize {
        self.bytes.len()
    }
}

impl<T> Default for GapStore<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            owned: 0,
        }
    }
}

impl<T: GapEntry> GapStore<T> {
    /// The bytes an entry's slot takes at most: std's B-tree node holds at least 5 of its 11
    /// keys and values beside 16 bytes of links, and an inner one 12 edges more for at least 6
    /// nodes below, so an entry's share of the nodes is under 2.6 times its key and value and 8
    /// bytes, which `3 · (key + value) + 16` bounds.
    pub(crate) const SLOT: usize = 3 * (size_of::<u16>() + size_of::<T>()) + 16;

    /// The bytes the store holds against the receive budget.
    pub(crate) fn held(&self) -> usize {
        self.entries.len() * Self::SLOT + self.owned
    }

    /// Whether the store holds `id`.
    pub(crate) fn contains(&self, id: MessageId) -> bool {
        self.entries.contains_key(&id.0)
    }

    /// Stores `entry`, which is not held, charging its slot and its bytes; returns false, and
    /// stores nothing, when the budget has no room for a limited `reservation`.
    pub(crate) fn insert(
        &mut self,
        entry: T,
        reservation: Reservation,
        budget: &mut ReceiveBudget,
    ) -> bool {
        if !budget.reserve(reservation, Self::SLOT + entry.owned_len()) {
            return false;
        }
        self.owned += entry.owned_len();
        let held = self.entries.insert(entry.id().0, entry);
        debug_assert!(held.is_none(), "an entry is stored once");
        true
    }

    /// Takes the entry of `id`, when the store holds it.
    pub(crate) fn pop(&mut self, id: MessageId) -> Option<T> {
        let entry = self.entries.remove(&id.0)?;
        self.owned -= entry.owned_len();
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use campfire_net_core::NetTick;

    /// An id's slot is `3 · (2 + 2) + 16` = 28 bytes: ids 5, 3 and 4 hold 84; the store takes
    /// only an id it holds, in any order, and holds nothing once each is taken. An id past the
    /// wrap of 65,536 is an id like any other.
    #[test]
    fn a_store_takes_its_ids_in_any_order_and_charges_each_slot() {
        assert_eq!(GapStore::<MessageId>::SLOT, 28);
        let mut store = GapStore::default();
        let mut budget = ReceiveBudget::new(usize::MAX);
        for id in [5, 3, u16::MAX] {
            assert!(store.insert(MessageId(id), Reservation::Limited, &mut budget));
        }
        assert_eq!(store.held(), 3 * 28);
        assert!(store.contains(MessageId(u16::MAX)) && !store.contains(MessageId(4)));
        assert_eq!(store.pop(MessageId(4)), None);
        for id in [u16::MAX, 3, 5] {
            assert_eq!(store.pop(MessageId(id)), Some(MessageId(id)));
        }
        assert_eq!(store.held(), 0);
    }

    /// A message's entry charges its slot and its bytes; with a byte less of room, a limited
    /// reservation stores nothing and a head one stores it anyway.
    #[test]
    fn a_limited_entry_past_the_room_is_refused() {
        let message = || ChannelMessage {
            tick: NetTick(0),
            bytes: Bytes::from_static(b"four"),
            message_id: Some(MessageId(1)),
        };
        let cost = GapStore::<ChannelMessage>::SLOT + 4;
        let mut store = GapStore::default();
        let mut budget = ReceiveBudget::new(cost - 1);
        assert!(!store.insert(message(), Reservation::Limited, &mut budget));
        assert_eq!(store.held(), 0);
        assert!(store.insert(message(), Reservation::Head, &mut budget));
        assert_eq!(store.held(), cost);
    }
}
