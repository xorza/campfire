use campfire_math::Num;

/// The fastest a unit of the mode moves, in meters a second, as its manifest caps it: what the
/// load gives the stat book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveSpeedCap(pub Num);
