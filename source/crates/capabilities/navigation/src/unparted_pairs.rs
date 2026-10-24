use bevy_ecs::resource::Resource;
use campfire_declarations::{Report, Role};

/// The contacts the last tick's collision left overlapping, past its passes, which a test of a
/// crowd holds at none (04-capabilities/navigation.md, Collision leaves no overlap).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnpartedPairs(usize);

impl UnpartedPairs {
    pub const fn get(self) -> usize {
        self.0
    }

    pub(crate) const fn set(&mut self, count: usize) {
        self.0 = count;
    }
}

impl Report for UnpartedPairs {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }
}
