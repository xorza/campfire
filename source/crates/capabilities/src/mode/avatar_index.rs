use serde::{Deserialize, Serialize};

/// An avatar of the mode, by its place among the avatars the mode depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AvatarIndex(u16);

impl AvatarIndex {
    pub(crate) fn new(index: usize) -> AvatarIndex {
        AvatarIndex(u16::try_from(index).expect("a mode's avatars fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
