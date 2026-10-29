use core::iter::FusedIterator;
use core::ops::RangeBounds;
use std::collections::{VecDeque, vec_deque};

#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;

use crate::net_tick::NetTick;
use crate::ticked::Ticked;

/// Entries ordered by tick from oldest to newest, one for each tick: the store under the
/// histories.
///
/// The ticks and the entries are two rings of one length, so a search reads the ticks alone, and
/// an entry pads to its own alignment, not to a tick's beside it.
#[derive(Debug)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub struct TickBuffer<E> {
    ticks: VecDeque<NetTick>,
    entries: VecDeque<E>,
}

impl<E> Default for TickBuffer<E> {
    fn default() -> Self {
        Self {
            ticks: VecDeque::new(),
            entries: VecDeque::new(),
        }
    }
}

impl<E> TickBuffer<E> {
    pub fn iter(&self) -> Iter<'_, E> {
        Iter {
            ticks: self.ticks.iter(),
            entries: self.entries.iter(),
        }
    }

    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    pub fn clear(&mut self) {
        self.ticks.clear();
        self.entries.clear();
    }

    /// The tick of the entry at `index`.
    pub fn tick(&self, index: usize) -> Option<NetTick> {
        self.ticks.get(index).copied()
    }

    /// The entry at `index`.
    pub fn get(&self, index: usize) -> Option<Ticked<&E>> {
        Some(Ticked::new(
            *self.ticks.get(index)?,
            self.entries.get(index)?,
        ))
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut E> {
        self.entries.get_mut(index)
    }

    pub fn front(&self) -> Option<Ticked<&E>> {
        self.get(0)
    }

    pub fn back(&self) -> Option<Ticked<&E>> {
        self.get(self.len().checked_sub(1)?)
    }

    pub fn front_mut(&mut self) -> Option<Ticked<&mut E>> {
        Some(Ticked::new(*self.ticks.front()?, self.entries.front_mut()?))
    }

    /// Moves the oldest entry to `tick`, which stays before every other.
    pub fn retick_front(&mut self, tick: NetTick) {
        debug_assert!(
            self.ticks.get(1).is_none_or(|next| tick < *next),
            "the oldest entry stays before every other"
        );
        if let Some(front) = self.ticks.front_mut() {
            *front = tick;
        }
    }

    /// The entries in `range` of indices.
    pub fn values_mut(&mut self, range: impl RangeBounds<usize>) -> vec_deque::IterMut<'_, E> {
        self.entries.range_mut(range)
    }

    /// The number of entries at or before `tick`.
    pub fn count_at_or_before(&self, tick: NetTick) -> usize {
        self.ticks.partition_point(|entry_tick| *entry_tick <= tick)
    }

    /// The number of entries before `tick`.
    pub fn count_before(&self, tick: NetTick) -> usize {
        self.ticks.partition_point(|entry_tick| *entry_tick < tick)
    }

    /// The index of the newest entry at or before `tick`.
    pub fn index_at_or_before(&self, tick: NetTick) -> Option<usize> {
        self.count_at_or_before(tick).checked_sub(1)
    }

    /// The index of the newest entry before `tick`.
    pub fn index_before(&self, tick: NetTick) -> Option<usize> {
        self.count_before(tick).checked_sub(1)
    }

    /// The index of the entry at `tick`.
    pub fn index_at(&self, tick: NetTick) -> Option<usize> {
        let index = self.count_before(tick);
        (self.ticks.get(index) == Some(&tick)).then_some(index)
    }

    /// Places `entry` at `tick` in order, replacing an entry at the same tick, and returns its
    /// index.
    pub fn insert(&mut self, tick: NetTick, entry: E) -> usize {
        let index = self.count_before(tick);
        if self.ticks.get(index) == Some(&tick) {
            self.entries[index] = entry;
        } else {
            self.ticks.insert(index, tick);
            self.entries.insert(index, entry);
        }
        index
    }

    /// Adds `entry` at `tick` as the newest, replacing one at the same tick.
    pub fn push(&mut self, tick: NetTick, entry: E) {
        if let Some(&newest_tick) = self.ticks.back() {
            debug_assert!(
                tick >= newest_tick,
                "a history is added to in tick order: {tick:?} after {newest_tick:?}"
            );
            if newest_tick == tick {
                *self.entries.back_mut().unwrap() = entry;
                return;
            }
        }
        self.ticks.push_back(tick);
        self.entries.push_back(entry);
    }

    pub fn push_front(&mut self, tick: NetTick, entry: E) {
        debug_assert!(
            self.ticks
                .front()
                .is_none_or(|front_tick| tick < *front_tick),
            "an entry goes in front only before every other"
        );
        self.ticks.push_front(tick);
        self.entries.push_front(entry);
    }

    /// Removes the entry at `index`.
    pub fn remove(&mut self, index: usize) -> Option<Ticked<E>> {
        Some(Ticked::new(
            self.ticks.remove(index)?,
            self.entries.remove(index)?,
        ))
    }

    pub fn pop_front(&mut self) -> Option<Ticked<E>> {
        Some(Ticked::new(
            self.ticks.pop_front()?,
            self.entries.pop_front()?,
        ))
    }

    /// Removes the oldest `count` entries.
    pub fn drain_front(&mut self, count: usize) -> Drain<'_, E> {
        Drain {
            ticks: self.ticks.drain(..count),
            entries: self.entries.drain(..count),
        }
    }

    /// Removes every entry after `tick`.
    pub fn truncate_after(&mut self, tick: NetTick) {
        let count = self.count_at_or_before(tick);
        self.ticks.truncate(count);
        self.entries.truncate(count);
    }

    /// Shifts every tick by `delta`. Ticks would saturate at 0 together, so the newest of the
    /// entries at or below it keeps tick 0, and `collapse` folds each older one into it, newest
    /// first, before it goes.
    pub fn shift(&mut self, delta: i32, mut collapse: impl FnMut(E, &mut E)) {
        let saturated = self
            .ticks
            .partition_point(|tick| i64::from(tick.0) + i64::from(delta) <= 0);
        if saturated > 1 {
            self.ticks.drain(..saturated - 1);
            let mut kept = self.entries.remove(saturated - 1).unwrap();
            for entry in self.entries.drain(..saturated - 1).rev() {
                collapse(entry, &mut kept);
            }
            self.entries.push_front(kept);
        }
        for tick in &mut self.ticks {
            *tick = *tick + delta;
        }
    }
}

impl<'a, E> IntoIterator for &'a TickBuffer<E> {
    type Item = Ticked<&'a E>;
    type IntoIter = Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<E> IntoIterator for TickBuffer<E> {
    type Item = Ticked<E>;
    type IntoIter = IntoIter<E>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            ticks: self.ticks.into_iter(),
            entries: self.entries.into_iter(),
        }
    }
}

/// The entries of a [`TickBuffer`], from oldest to newest.
#[derive(Debug, Clone)]
pub struct Iter<'a, E> {
    ticks: vec_deque::Iter<'a, NetTick>,
    entries: vec_deque::Iter<'a, E>,
}

impl<'a, E> Iterator for Iter<'a, E> {
    type Item = Ticked<&'a E>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(*self.ticks.next()?, self.entries.next()?))
    }

    /// Skips both rings at once, as their own iterators do, instead of one entry at a time.
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        Some(Ticked::new(*self.ticks.nth(n)?, self.entries.nth(n)?))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ticks.size_hint()
    }
}

impl<E> DoubleEndedIterator for Iter<'_, E> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(
            *self.ticks.next_back()?,
            self.entries.next_back()?,
        ))
    }
}

impl<E> ExactSizeIterator for Iter<'_, E> {}

impl<E> FusedIterator for Iter<'_, E> {}

/// The entries of a [`TickBuffer`] it gave up, from oldest to newest.
#[derive(Debug)]
pub struct IntoIter<E> {
    ticks: vec_deque::IntoIter<NetTick>,
    entries: vec_deque::IntoIter<E>,
}

impl<E> Iterator for IntoIter<E> {
    type Item = Ticked<E>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(self.ticks.next()?, self.entries.next()?))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ticks.size_hint()
    }
}

impl<E> DoubleEndedIterator for IntoIter<E> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(
            self.ticks.next_back()?,
            self.entries.next_back()?,
        ))
    }
}

impl<E> ExactSizeIterator for IntoIter<E> {}

impl<E> FusedIterator for IntoIter<E> {}

/// The oldest entries of a [`TickBuffer`], removed as it yields them.
#[derive(Debug)]
pub struct Drain<'a, E> {
    ticks: vec_deque::Drain<'a, NetTick>,
    entries: vec_deque::Drain<'a, E>,
}

impl<E> Iterator for Drain<'_, E> {
    type Item = Ticked<E>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(self.ticks.next()?, self.entries.next()?))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ticks.size_hint()
    }
}

impl<E> DoubleEndedIterator for Drain<'_, E> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(Ticked::new(
            self.ticks.next_back()?,
            self.entries.next_back()?,
        ))
    }
}

impl<E> ExactSizeIterator for Drain<'_, E> {}

impl<E> FusedIterator for Drain<'_, E> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(buffer: &TickBuffer<char>) -> Vec<(u32, char)> {
        buffer
            .iter()
            .map(|entry| (entry.tick.0, *entry.value))
            .collect()
    }

    /// Entries at 1, 2 and 5 shifted by -3: 1 and 2 would both saturate at 0, so 2 keeps it
    /// with 1 folded in, and 5 moves to 2. Each tick stays unique.
    #[test]
    fn a_shift_keeps_keys_unique() {
        let mut buffer = TickBuffer::default();
        for (tick, entry) in [(1, 'a'), (2, 'b'), (5, 'c')] {
            buffer.push(NetTick(tick), entry);
        }
        let mut folded = Vec::new();
        buffer.shift(-3, |older, kept| folded.push((older, *kept)));

        assert_eq!(entries(&buffer), [(0, 'b'), (2, 'c')]);
        assert_eq!(folded, [('a', 'b')]);

        buffer.shift(4, |_, _| unreachable!("nothing saturates"));
        assert_eq!(entries(&buffer), [(4, 'b'), (6, 'c')]);
    }

    /// An insert places each entry in tick order and replaces one at its tick; a lookup, a
    /// drain of the front and a cut after a tick keep the two rings of one length, and every
    /// iterator tells its exact length.
    #[test]
    fn the_rings_stay_in_step() {
        let mut buffer = TickBuffer::default();
        for (tick, entry) in [(5, 'c'), (1, 'a'), (3, 'b'), (3, 'B'), (7, 'd')] {
            buffer.insert(NetTick(tick), entry);
        }
        assert_eq!(entries(&buffer), [(1, 'a'), (3, 'B'), (5, 'c'), (7, 'd')]);
        assert_eq!(
            [0, 1, 3, 4, 8].map(|tick| buffer.index_at_or_before(NetTick(tick))),
            [None, Some(0), Some(1), Some(1), Some(3)]
        );
        assert_eq!(buffer.index_at(NetTick(4)), None);
        assert_eq!(buffer.back(), Some(Ticked::new(NetTick(7), &'d')));
        assert_eq!(buffer.iter().len(), 4);
        assert_eq!(buffer.iter().nth(2), Some(Ticked::new(NetTick(5), &'c')));
        assert_eq!(buffer.iter().skip(3).len(), 1);
        assert_eq!(buffer.iter().next_back(), buffer.back());

        let drained: Vec<_> = buffer.drain_front(2).collect();
        assert_eq!(
            drained,
            [Ticked::new(NetTick(1), 'a'), Ticked::new(NetTick(3), 'B')]
        );
        buffer.retick_front(NetTick(6));
        assert_eq!(buffer.remove(1), Some(Ticked::new(NetTick(7), 'd')));
        assert_eq!(buffer.remove(1), None);
        buffer.truncate_after(NetTick(6));
        assert_eq!(entries(&buffer), [(6, 'c')]);
        assert_eq!(buffer.pop_front(), Some(Ticked::new(NetTick(6), 'c')));
        assert!(buffer.is_empty() && buffer.front().is_none());
        assert_eq!(buffer.into_iter().len(), 0);
    }
}
