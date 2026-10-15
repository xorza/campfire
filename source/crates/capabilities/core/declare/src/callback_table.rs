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
    fn default() -> CallbackTable<C, N> {
        CallbackTable([const { None }; N])
    }
}

#[cfg(test)]
mod tests {
    use crate::callback_table::CallbackTable;

    #[test]
    fn a_place_takes_one_callback_and_the_rest_stay_empty() {
        let mut table = CallbackTable::<u8, 3>::default();
        table.register(2, 7);
        table.register(0, 5);
        assert_eq!([0, 1, 2].map(|at| table.get(at)), [Some(5), None, Some(7)]);
        assert_eq!(table.iter().collect::<Vec<_>>(), [5, 7]);
    }

    #[test]
    #[should_panic(expected = "a place of a registry takes one callback")]
    fn a_place_refuses_a_second_callback() {
        let mut table = CallbackTable::<u8, 1>::default();
        table.register(0, 1);
        table.register(0, 2);
    }
}
