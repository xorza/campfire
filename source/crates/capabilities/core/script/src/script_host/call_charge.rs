use std::cell::Cell;
use std::rc::Rc;

/// The operations a call's native work counts beside Rhai's own: the view's list methods add a
/// step for each row they visit, and the host counts them toward the call's limit and its budget
/// (02-engine-core.md, A list costs a step a row). The view and the host share it.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallCharge(Rc<Cell<u64>>);

impl CallCharge {
    /// Adds `steps`.
    pub(crate) fn add(&self, steps: u64) {
        self.0.set(self.0.get().saturating_add(steps));
    }

    pub(crate) fn get(&self) -> u64 {
        self.0.get()
    }

    /// Starts a call's count at none.
    pub(crate) fn clear(&self) {
        self.0.set(0);
    }
}
