use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a supply module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum SupplyDropCause {
    /// A dock that holds no box.
    NoBoxes,
    /// A number past what the engine holds.
    BadNumber,
    /// A gatherer, or a unit a building spawns, that does not move.
    NoWalker,
    /// A unit a building of no production spawns, which has no exit to leave.
    NoExit,
    /// A spawned unit that no file defines.
    NoObject,
    /// A kept spawn of no life, which never dies to come back.
    Unliving,
    /// A gatherer whose gather's name, of its kind and its type's, passes the bytes a declared
    /// name holds.
    LongName,
    /// An `OCLUpdate`, which creates objects over time, as America's supply drop zone sends a
    /// cargo plane of crates (09-zero-hour.md, A supply drop zone's crates are lost).
    ObjectsOverTime,
    /// A hack's cash at a veterancy past regular that is not its regular cash, as the import
    /// gives no unit a veterancy.
    HackVeterancy,
    /// A hack's `XpPerCashUpdate`, as the import gives no unit a veterancy.
    HackExperience,
    /// A hack's `CashUpdateDelayFast`, its delay in a building, as the import puts no unit in
    /// one.
    HackInBuilding,
    /// A hack's pack spread by its `PackUnpackVariationFactor`, as a channel's wind-down does not
    /// spread (04-capabilities/actions.md, A channel with no end).
    PackSpread,
    /// A leader's spawns, which take its orders in place of their own `MobMemberSlavedUpdate`:
    /// its catch-up, its crisis bail and its self-tasking go, and no spawn of a hurt leader goes
    /// prone (09-zero-hour.md, The import converts a mob).
    LedSpawns,
    /// A spawn whose object gives no unit type, as one of a locomotor no file defines, which the
    /// import refuses as it refuses the type.
    NoType,
}
