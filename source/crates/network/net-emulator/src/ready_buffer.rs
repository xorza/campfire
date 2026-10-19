//! A queue of items that each become ready at a key.

use core::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

/// Items that each become ready at a key, such as a tick or an instant.
///
/// [`pop_ready`](Self::pop_ready) hands the ready items out in key order, and items with equal keys
/// in the order they were pushed.
#[derive(Clone, Debug)]
pub(crate) struct ReadyBuffer<K, T> {
    heap: BinaryHeap<Reverse<Entry<K, T>>>,
    pushed: u64,
}

#[derive(Clone, Debug)]
struct Entry<K, T> {
    key: K,
    order: u64,
    item: T,
}

impl<K: Ord, T> PartialEq for Entry<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<K: Ord, T> Eq for Entry<K, T> {}

impl<K: Ord, T> PartialOrd for Entry<K, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord, T> Ord for Entry<K, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key).then(self.order.cmp(&other.order))
    }
}

impl<K: Ord, T> Default for ReadyBuffer<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord, T> ReadyBuffer<K, T> {
    pub(crate) const fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            pushed: 0,
        }
    }

    /// Queues `item` to become ready at `key`.
    pub(crate) fn push(&mut self, key: K, item: T) {
        self.heap.push(Reverse(Entry {
            key,
            order: self.pushed,
            item,
        }));
        self.pushed += 1;
    }

    /// The key the next item becomes ready at.
    pub(crate) fn next_key(&self) -> Option<&K> {
        self.heap.peek().map(|Reverse(entry)| &entry.key)
    }

    /// Removes and returns the next item whose key is at or before `now`.
    pub(crate) fn pop_ready(&mut self, now: &K) -> Option<T> {
        if self
            .heap
            .peek()
            .is_some_and(|Reverse(entry)| entry.key <= *now)
        {
            self.heap.pop().map(|Reverse(entry)| entry.item)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_leave_in_key_order_once_ready() {
        let mut buffer = ReadyBuffer::new();
        buffer.push(3, 'c');
        buffer.push(1, 'a');
        buffer.push(2, 'b');

        assert_eq!(buffer.pop_ready(&0), None);
        assert_eq!(buffer.pop_ready(&2), Some('a'));
        assert_eq!(buffer.pop_ready(&2), Some('b'));
        assert_eq!(buffer.pop_ready(&2), None);
        assert_eq!(buffer.pop_ready(&3), Some('c'));
        assert_eq!(buffer.pop_ready(&3), None);
    }

    /// A binary heap gives no order to equal keys: with the key alone, 0..8 pushed under one key
    /// popped as `[0, 2, 6, 5, 7, 4, 1, 3]`.
    #[test]
    fn items_with_equal_keys_leave_in_push_order() {
        let mut buffer = ReadyBuffer::new();
        for item in 0..8 {
            buffer.push(5, item);
        }
        buffer.push(4, 100);

        let popped = core::iter::from_fn(|| buffer.pop_ready(&5)).collect::<Vec<_>>();
        assert_eq!(popped, [100, 0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
