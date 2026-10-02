/// Operations a group of calls may still run in the running tick. Each call draws from the
/// budget of its group, so one group's calls cannot spend another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget(u64);

impl Budget {
    pub const fn new(operations: u64) -> Budget {
        Budget(operations)
    }

    pub const fn left(self) -> u64 {
        self.0
    }

    /// Takes `operations`; a call that ran out takes one past what was left.
    pub(crate) const fn spend(&mut self, operations: u64) {
        self.0 = self.0.saturating_sub(operations);
    }
}
