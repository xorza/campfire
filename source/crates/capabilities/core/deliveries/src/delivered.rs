use campfire_actions::Reached;
use campfire_values::Hit;

use crate::delivering::Delivering;

/// What a delivery `by` reached, as `hit` says: a unit it hit, whose hook is `on_hit`, or its end,
/// whose hook is `on_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    pub by: Delivering,
    pub reach: Reached,
    pub hit: Hit,
}
