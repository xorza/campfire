use bevy_ecs::resource::Resource;
use campfire_declarations::{Report, Role};

/// The walkers that stood stuck `Progress::ask_after` as the last tick steered, each of which asked
/// its route again or stopped, which a check of a crowd counts (04-capabilities/navigation.md, A
/// stuck walker plans again, then stops).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StalledWalkers(usize);

impl StalledWalkers {
    pub const fn get(self) -> usize {
        self.0
    }

    pub(crate) const fn set(&mut self, count: usize) {
        self.0 = count;
    }
}

impl Report for StalledWalkers {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }
}
