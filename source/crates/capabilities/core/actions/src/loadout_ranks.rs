use campfire_declarations::Fact;

/// The ranks of every loadout entry: those of the slot kind a choice of loadout entries fills,
/// which the mode makes, 1 with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadoutRanks(pub u8);

impl Fact for LoadoutRanks {}
