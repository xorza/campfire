use crate::net_tick::NetTick;

/// A value at a tick: an entry of a history, the shape every tick-and-value pair takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticked<T> {
    pub tick: NetTick,
    pub value: T,
}

impl<T> Ticked<T> {
    pub const fn new(tick: NetTick, value: T) -> Self {
        Self { tick, value }
    }

    /// The same tick, with a reference to the value.
    pub const fn as_ref(&self) -> Ticked<&T> {
        Ticked {
            tick: self.tick,
            value: &self.value,
        }
    }

    /// The same tick, with `f` of the value.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Ticked<U> {
        Ticked {
            tick: self.tick,
            value: f(self.value),
        }
    }
}

impl<T: Clone> Ticked<&T> {
    /// The same tick, with a clone of the value.
    #[must_use]
    pub fn cloned(self) -> Ticked<T> {
        self.map(Clone::clone)
    }
}
