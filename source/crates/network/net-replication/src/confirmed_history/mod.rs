use crate::history_state::HistoryState;
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_core::Iter as EntryIter;
use campfire_net_core::NetTick;
use campfire_net_core::TickBuffer;
use campfire_net_core::Ticked;
use core::iter::FusedIterator;

/// Stores authoritative component states received from the remote.
///
/// The entries are ordered from oldest to newest. An entry is either an explicit authoritative
/// update or removal received from replication, or an unchanged anchor when prediction or
/// interpolation has external proof that the authoritative state did not change at a tick.
///
/// An anchor holds its tick alone: its state is the newest explicit one at or before it, which a
/// binary search finds. A late explicit update inserted before an anchor therefore becomes the
/// anchor's state, and the front entry is always explicit.
///
/// `Changed<ConfirmedHistory<C>>` means an authoritative update or removal arrived. Carrying a
/// value forward to a later tick and pruning old entries change no authoritative state, so they
/// do not mark the component changed.
#[derive(Component, Debug, Reflect)]
pub struct ConfirmedHistory<C> {
    /// The tick of every entry, explicit or an anchor.
    entries: TickBuffer<()>,
    /// The explicit states, each at an entry's tick.
    explicit: TickBuffer<HistoryState<C>>,
}

impl<C> Default for ConfirmedHistory<C> {
    fn default() -> Self {
        Self {
            entries: TickBuffer::default(),
            explicit: TickBuffer::default(),
        }
    }
}

impl<C> ConfirmedHistory<C> {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates the present values from oldest to most recent, one for each entry.
    pub fn iter(&self) -> Iter<'_, C> {
        Iter {
            entries: self.entries.iter(),
            explicit: self.explicit.iter().peekable(),
            current: None,
        }
    }

    /// The number of entries at or before `tick`.
    pub fn count_at_or_before(&self, tick: NetTick) -> usize {
        self.entries.count_at_or_before(tick)
    }

    /// The n-th oldest tick in the history.
    pub fn get_nth_tick(&self, n: usize) -> Option<NetTick> {
        self.entries.tick(n)
    }

    /// The n-th oldest resolved state in the history.
    pub fn get_nth_state(&self, n: usize) -> Option<Ticked<&HistoryState<C>>> {
        let tick = self.entries.tick(n)?;
        Some(Ticked::new(tick, self.get_state_at_or_before(tick)?))
    }

    /// The oldest present value in the history.
    pub fn start_present(&self) -> Option<Ticked<&C>> {
        self.get_nth_present(0)
    }

    /// The most recent present value in the history.
    pub fn newest_present(&self) -> Option<Ticked<&C>> {
        self.get_nth_present(self.len().checked_sub(1)?)
    }

    /// The n-th oldest entry if it is a present value.
    pub fn get_nth_present(&self, n: usize) -> Option<Ticked<&C>> {
        let state = self.get_nth_state(n)?;
        Some(Ticked::new(state.tick, state.value.value()?))
    }

    /// The latest authoritative state at or before `tick`.
    pub fn get_state_at_or_before(&self, tick: NetTick) -> Option<&HistoryState<C>> {
        let index = self.explicit.index_at_or_before(tick)?;
        self.explicit.get(index).map(|state| state.value)
    }

    /// The latest authoritative state strictly before `tick`.
    pub fn get_state_before(&self, tick: NetTick) -> Option<&HistoryState<C>> {
        let index = self.explicit.index_before(tick)?;
        self.explicit.get(index).map(|state| state.value)
    }

    /// Whether the component was removed at a tick after `after`, through `through`. A removal
    /// is an explicit entry, so only the explicit entries between the two ticks are read.
    pub fn has_removal_between(&self, after: NetTick, through: NetTick) -> bool {
        self.explicit
            .iter()
            .skip(self.explicit.count_at_or_before(after))
            .take_while(|state| state.tick <= through)
            .any(|state| matches!(state.value, HistoryState::Removed))
    }

    /// The authoritative state exactly at `tick`.
    pub fn get_state_at(&self, tick: NetTick) -> Option<&HistoryState<C>> {
        self.entries.index_at(tick)?;
        self.get_state_at_or_before(tick)
    }

    /// Inserts the explicit `state` at `tick`, replacing an entry there.
    fn insert_explicit(&mut self, tick: NetTick, state: HistoryState<C>) {
        self.entries.insert(tick, ());
        self.explicit.insert(tick, state);
    }

    /// Inserts an anchor at `tick`, replacing an explicit entry there: its state is the one
    /// before, which must exist.
    fn insert_anchor(&mut self, tick: NetTick) {
        debug_assert!(
            self.explicit.index_before(tick).is_some(),
            "an unchanged entry follows a state"
        );
        self.entries.insert(tick, ());
        if let Some(index) = self.explicit.index_at(tick) {
            self.explicit.remove(index);
        }
    }

    /// Insert an explicit present authoritative value without unchanged-state
    /// compression.
    pub fn insert_present_explicit(&mut self, tick: NetTick, value: C) {
        self.insert_explicit(tick, HistoryState::Updated(value));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.explicit.clear();
    }

    /// Shift all stored ticks by `delta`. Entries that would saturate at tick 0 together keep only
    /// the newest, which takes the newest explicit state among them.
    pub fn shift(&mut self, delta: i32) {
        self.entries.shift(delta, |(), ()| {});
        self.explicit.shift(delta, |_, _| {});
    }
}

impl<C: PartialEq> ConfirmedHistory<C> {
    /// Insert an authoritative state while preserving tick order.
    ///
    /// If the state is equal to the effective authoritative state immediately
    /// before `tick`, the entry is stored as an unchanged anchor.
    pub fn insert(&mut self, tick: NetTick, state: HistoryState<C>) {
        if self
            .get_state_before(tick)
            .is_some_and(|prev| prev == &state)
        {
            self.insert_anchor(tick);
        } else {
            self.insert_explicit(tick, state);
        }
    }

    /// Insert a present authoritative value while preserving tick order.
    pub fn insert_present(&mut self, tick: NetTick, value: C) {
        self.insert(tick, HistoryState::Updated(value));
    }

    /// Insert an authoritative removal while preserving tick order.
    pub fn insert_removed(&mut self, tick: NetTick) {
        self.insert(tick, HistoryState::Removed);
    }
}

impl<C> ConfirmedHistory<C> {
    /// Add an unchanged sample at `tick`.
    ///
    /// Use this when another system has proven that the authoritative component
    /// state did not change at `tick` even though no explicit component update
    /// was received.
    pub fn add_unchanged(&mut self, tick: NetTick) -> bool {
        if self.get_state_at(tick).is_some() || self.get_state_before(tick).is_none() {
            return false;
        }
        self.insert_anchor(tick);
        true
    }

    /// Advance the newest present value to `tick` for interpolation.
    ///
    /// Use this only when `tick` comes from a monotonic completeness signal,
    /// such as the latest completed mutate tick. Consecutive unchanged ticks
    /// are stored as separate anchors so user interpolation systems can
    /// align multiple component histories at the same confirmed tick.
    pub fn push_unchanged(&mut self, tick: NetTick) -> Option<NetTick> {
        let newest = self.newest_present()?.tick;
        if tick <= newest {
            return None;
        }
        self.add_unchanged(tick).then_some(newest)
    }

    /// Clear all states strictly older than `tick`, keeping the state at `tick` as the front
    /// entry. The dropped entries' state moves to the front, so nothing is cloned.
    pub fn clear_until_tick(&mut self, tick: NetTick) {
        let count = self.entries.count_before(tick);
        if count == 0 {
            return;
        }
        self.entries.drain_front(count);
        let explicit_count = self.explicit.count_before(tick);
        let dropped = self
            .explicit
            .drain_front(explicit_count)
            .last()
            .expect("the front entry is explicit");
        if self.explicit.front().is_none_or(|front| front.tick != tick) {
            self.explicit.push_front(tick, dropped.value);
        }
        if self.entries.front().is_none_or(|front| front.tick != tick) {
            self.entries.push_front(tick, ());
        }
    }
}

/// The iterator contains the present values from oldest to most recent.
impl<'a, C> IntoIterator for &'a ConfirmedHistory<C> {
    type Item = Ticked<&'a C>;
    type IntoIter = Iter<'a, C>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The present values of a [`ConfirmedHistory`], one for each entry, from oldest to most recent.
#[derive(Debug)]
pub struct Iter<'a, C> {
    entries: EntryIter<'a, ()>,
    explicit: core::iter::Peekable<EntryIter<'a, HistoryState<C>>>,
    current: Option<&'a HistoryState<C>>,
}

impl<'a, C> Iterator for Iter<'a, C> {
    type Item = Ticked<&'a C>;

    fn next(&mut self) -> Option<Self::Item> {
        for entry in self.entries.by_ref() {
            if let Some(state) = self.explicit.next_if(|state| state.tick == entry.tick) {
                self.current = Some(state.value);
            }
            if let Some(HistoryState::Updated(value)) = self.current {
                return Some(Ticked::new(entry.tick, value));
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.entries.size_hint().1)
    }
}

impl<C> FusedIterator for Iter<'_, C> {}

#[cfg(test)]
mod tests;
