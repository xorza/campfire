use bevy_reflect::Reflect;
use core::fmt::Debug;

/// Stores a past value in the history buffer
#[derive(Debug, PartialEq, Eq, Clone, Default, Reflect)]
pub enum HistoryState<R> {
    #[default]
    /// the value just got removed
    Removed,
    /// the value got updated
    Updated(R),
}

impl<R> HistoryState<R> {
    /// Returns true if the value exists in this state.
    pub const fn is_present(&self) -> bool {
        matches!(self, Self::Updated(_))
    }

    /// The inner value if present.
    pub const fn value(&self) -> Option<&R> {
        match self {
            Self::Updated(value) => Some(value),
            Self::Removed => None,
        }
    }

    /// The inner value if present.
    pub const fn value_mut(&mut self) -> Option<&mut R> {
        match self {
            Self::Updated(value) => Some(value),
            Self::Removed => None,
        }
    }

    /// The inner value if present.
    pub fn into_value(self) -> Option<R> {
        match self {
            Self::Updated(value) => Some(value),
            Self::Removed => None,
        }
    }
}
