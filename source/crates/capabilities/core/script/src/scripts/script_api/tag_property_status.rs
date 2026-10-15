use campfire_values::TagProperty;

use crate::scripts::status::Status;

/// Whether the release honours a property a tag may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPropertyStatus {
    pub property: TagProperty,
    pub status: Status,
}
