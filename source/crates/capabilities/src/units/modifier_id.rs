use serde::{Deserialize, Serialize};

/// A modifier, by its place in the match's modifier book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModifierId(u16);

impl ModifierId {
    /// The modifier at `index` of the book.
    pub(crate) fn nth(index: usize) -> ModifierId {
        ModifierId(u16::try_from(index).expect("modifiers fit u16"))
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::units::modifier_id::ModifierId;

    impl ModifierId {
        pub(crate) const fn new(index: u16) -> ModifierId {
            ModifierId(index)
        }
    }
}
