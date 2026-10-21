/// The callbacks of a registry, one or none in each of its `N` places, which its keys name: a
/// place takes one callback, once.
#[derive(Debug, Clone, Copy)]
pub struct CallbackTable<C, const N: usize>([Option<C>; N]);

impl<C: Copy, const N: usize> CallbackTable<C, N> {
    /// Puts `callback` in place `at`, which holds none yet.
    pub fn register(&mut self, at: usize, callback: C) {
        let slot = &mut self.0[at];
        assert!(slot.is_none(), "a place of a registry takes one callback");
        *slot = Some(callback);
    }

    /// The callback in place `at`, when one was registered there.
    pub const fn get(&self, at: usize) -> Option<C> {
        self.0[at]
    }

    /// Each registered callback, in the order of their places.
    pub fn iter(&self) -> impl Iterator<Item = C> {
        self.0.into_iter().flatten()
    }
}

impl<C, const N: usize> Default for CallbackTable<C, N> {
    fn default() -> Self {
        Self([const { None }; N])
    }
}

#[cfg(test)]
mod tests;
