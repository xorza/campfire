use crate::scripts::script_api::status::Status;
use crate::units::tag_property::TagProperty;

/// Whether the release honours a property a tag may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPropertyStatus {
    pub property: TagProperty,
    pub status: Status,
}
