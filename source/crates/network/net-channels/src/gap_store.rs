use std::collections::VecDeque;

use crate::channel_message::ChannelMessage;
use crate::message_id::MessageId;
use crate::receive_budget::{ReceiveBudget, Reservation};

/// What a reliable channel holds past a gap after the id it waits for, sorted by each entry's
/// distance from that id: an entry and the bytes it owns are charged to the receive budget, its
/// slot in the store's capacity included, and the store gives its capacity back once the gap
/// closes (11-net-stack/transport.md, Each link's `Channels` has one receive budget).
#[derive(Debug)]
pub(crate) struct GapStore<T> {
    entries: VecDeque<T>,
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
            entries: VecDeque::new(),
            owned: 0,
        }
    }
}

impl<T: GapEntry> GapStore<T> {
    /// The bytes the store holds against the receive budget.
    pub(crate) fn held(&self) -> usize {
        self.entries.capacity() * size_of::<T>() + self.owned
    }

    /// Whether the store holds `id`, which is not older than `from`.
    pub(crate) fn contains(&self, from: MessageId, id: MessageId) -> bool {
        self.position(from, id).is_ok()
    }

    /// Where `id` is, or would go, among the entries ahead of `from`.
    fn position(&self, from: MessageId, id: MessageId) -> Result<usize, usize> {
        self.entries
            .binary_search_by_key(&(id - from), |entry| entry.id() - from)
    }

    /// Stores `entry`, which is ahead of `from` and not held, charging its bytes and the slots the
    /// store grows by, a doubling of its capacity when it is full; returns false, and stores
    /// nothing, when the budget has no room for a limited `reservation`.
    pub(crate) fn insert(
        &mut self,
        from: MessageId,
        entry: T,
        reservation: Reservation,
        budget: &mut ReceiveBudget,
    ) -> bool {
        let at = self
            .position(from, entry.id())
            .expect_err("an entry is stored once");
        let len = self.entries.len();
        let grow = if len == self.entries.capacity() {
            len.max(1)
        } else {
            0
        };
        if !budget.reserve(reservation, grow * size_of::<T>() + entry.owned_len()) {
            return false;
        }
        self.entries.reserve_exact(grow);
        debug_assert!(
            grow == 0 || self.entries.capacity() == len + grow,
            "the store grows by the slots it charged"
        );
        self.owned += entry.owned_len();
        self.entries.insert(at, entry);
        true
    }

    /// Takes the first entry, when it is `id`'s; the capacity goes with the last.
    pub(crate) fn pop(&mut self, id: MessageId) -> Option<T> {
        if self.entries.front()?.id() != id {
            return None;
        }
        let entry = self.entries.pop_front()?;
        self.owned -= entry.owned_len();
        if self.entries.is_empty() {
            self.entries = VecDeque::new();
        }
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use campfire_net_core::NetTick;

    /// Ids 5, 3 and 4 ahead of 2 sort as 3, 4, 5; the store charges a slot as it doubles from 0 to
    /// 1, 2 and 4, so 1 + 1 + 2 slots of a 2-byte id, 8 bytes; it pops only its first id, and
    /// gives its capacity back with the last.
    #[test]
    fn a_store_sorts_by_distance_charges_its_growth_and_frees_it_once_empty() {
        let from = MessageId(2);
        let mut store = GapStore::default();
        let mut budget = ReceiveBudget::new(usize::MAX);
        for id in [5, 3, 4] {
            assert!(store.insert(from, MessageId(id), Reservation::Limited, &mut budget));
        }
        assert_eq!(store.held(), 4 * size_of::<MessageId>());
        assert!(store.contains(from, MessageId(4)) && !store.contains(from, MessageId(6)));
        assert_eq!(store.pop(MessageId(4)), None);
        for id in [3, 4, 5] {
            assert_eq!(store.pop(MessageId(id)), Some(MessageId(id)));
        }
        assert_eq!(store.held(), 0);
    }

    /// Ids past the wrap of 65,536 sort by their distance from `from`, not their value.
    #[test]
    fn ids_past_the_wrap_sort_after_the_ones_before_it() {
        let from = MessageId(u16::MAX - 1);
        let mut store = GapStore::default();
        let mut budget = ReceiveBudget::new(usize::MAX);
        for id in [1, u16::MAX] {
            assert!(store.insert(from, MessageId(id), Reservation::Limited, &mut budget));
        }
        assert_eq!(store.pop(MessageId(1)), None);
        assert_eq!(store.pop(MessageId(u16::MAX)), Some(MessageId(u16::MAX)));
        assert_eq!(store.pop(MessageId(1)), Some(MessageId(1)));
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
        let cost = size_of::<ChannelMessage>() + 4;
        let mut store = GapStore::default();
        let mut budget = ReceiveBudget::new(cost - 1);
        assert!(!store.insert(MessageId(0), message(), Reservation::Limited, &mut budget));
        assert_eq!(store.held(), 0);
        assert!(store.insert(MessageId(0), message(), Reservation::Head, &mut budget));
        assert_eq!(store.held(), cost);
    }
}
