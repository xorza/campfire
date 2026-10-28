use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a special power's button, which converts to no cast (09-zero-hour.md, A
/// power is a cast of its building).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum PowerDropCause {
    /// No file defines its power, or its building holds no `OCLSpecialPower` of it.
    NoPower,
    /// Its power is of another module than an `OCLSpecialPower`'s list made where its player
    /// aims, as a delivery by a transport, a unit's or a bounty.
    NotAtLocation,
    /// A list of its power makes other than one object of a single-burst `AutoHealBehavior`.
    NoBurst,
    /// Its burst heals objects of another `KindOf` than `VEHICLE`, which no tag of the mode marks.
    Kind,
    /// Its power needs no science, whose perk gives its building its tag.
    NoScience,
    /// Its name, as the package names it, passes the bytes a name holds.
    LongName,
}
