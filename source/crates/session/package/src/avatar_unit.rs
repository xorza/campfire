use campfire_declarations::UnitTypeData;
use campfire_values::TextId;

/// An avatar's unit type, and the message of the name players see for it.
#[derive(Debug)]
pub struct AvatarUnit {
    pub name: TextId,
    pub unit: UnitTypeData,
}
