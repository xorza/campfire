use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum ButtonDropCause {
    /// No file defines the button, or its object or its upgrade, or an object its prerequisites
    /// name.
    NoObject,
    /// It trains an object that does not walk, or variations one of which does not.
    NoWalker,
    /// It builds an object of no body, as a small structure (09-zero-hour.md, A cylinder structure
    /// builds as a circle).
    NoBody,
    /// It trains or researches at an object of no `ProductionUpdate`, or of a queue past 1 to
    /// 255.
    NoQueue,
    /// Its object, or an object its prerequisites name, gives no unit type.
    NoType,
    /// Its object's or its upgrade's build time is below 0, or not finite.
    BadTime,
    /// Its action's name, of its kind and its type's or its upgrade's, or its upgrade's own,
    /// passes the bytes a declared name holds.
    LongName,
}
