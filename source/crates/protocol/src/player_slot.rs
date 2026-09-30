/// A player's place in the session header, in join order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerSlot(u32);

impl PlayerSlot {
    pub const fn new(index: u32) -> PlayerSlot {
        PlayerSlot(index)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}
