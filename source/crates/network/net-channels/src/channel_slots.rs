use campfire_net_core::NetId;
use core::ops::Index;

/// The channels of one side of a transport, by the dense net id that the channel registry gives
/// each channel: a lookup is an index, not a hash.
#[derive(Debug)]
pub(crate) struct ChannelSlots<T> {
    slots: Vec<Option<T>>,
}

impl<T> Default for ChannelSlots<T> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<T> ChannelSlots<T> {
    pub(crate) fn get(&self, id: NetId) -> Option<&T> {
        self.slots.get(usize::from(id.get()))?.as_ref()
    }

    pub(crate) fn get_mut(&mut self, id: NetId) -> Option<&mut T> {
        self.slots.get_mut(usize::from(id.get()))?.as_mut()
    }

    /// Puts `channel` at `id`, in place of a channel already there.
    pub(crate) fn insert(&mut self, id: NetId, channel: T) {
        let index = usize::from(id.get());
        if self.slots.len() <= index {
            self.slots.resize_with(index + 1, || None);
        }
        self.slots[index] = Some(channel);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.slots.iter_mut().flatten()
    }
}

impl<T> Index<NetId> for ChannelSlots<T> {
    type Output = T;

    fn index(&self, id: NetId) -> &T {
        self.get(id)
            .expect("a channel id names a channel of this transport")
    }
}

impl<'a, T> IntoIterator for &'a ChannelSlots<T> {
    type Item = &'a T;
    type IntoIter = core::iter::Flatten<core::slice::Iter<'a, Option<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.iter().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Channels 1 and 4 fill slots 1 and 4; slots 0, 2 and 3 stay empty, and a later insert at 1
    /// replaces its channel.
    #[test]
    fn a_channel_lives_at_its_id() {
        let mut slots = ChannelSlots::default();
        slots.insert(NetId::new(4), 'd');
        slots.insert(NetId::new(1), 'a');
        assert_eq!(slots.get(NetId::new(1)), Some(&'a'));
        assert_eq!(slots.get(NetId::new(4)), Some(&'d'));
        assert_eq!(slots.get(NetId::new(2)), None);
        assert_eq!(slots.get(NetId::new(9)), None);
        slots.insert(NetId::new(1), 'b');
        assert_eq!(slots[NetId::new(1)], 'b');
        assert_eq!(slots.iter().copied().collect::<Vec<_>>(), ['b', 'd']);
        *slots.get_mut(NetId::new(4)).unwrap() = 'e';
        assert_eq!(
            (&slots).into_iter().copied().collect::<Vec<_>>(),
            ['b', 'e']
        );
    }
}
