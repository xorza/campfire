use campfire_declarations::Fact;
use campfire_units::PoolId;

/// The mode's life pool, the one a unit dies at zero of, which the module that kills units names;
/// none when the mode names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifePool(pub Option<PoolId>);

impl Fact for LifePool {}
