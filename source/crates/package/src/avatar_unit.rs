use campfire_capabilities::UnitTypeFile;
use campfire_content::MessageId;

/// An avatar's unit type, and the message of the name players see for it.
#[derive(Debug)]
pub struct AvatarUnit {
    pub name: MessageId,
    pub unit: UnitTypeFile,
}
