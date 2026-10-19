/// Unique identifier for a room.
///
/// The [`RoomId`] must be allocated via the `RoomAllocator` resource.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    bevy_reflect::Reflect,
)]
pub struct RoomId(pub(crate) u16);

impl From<RoomId> for usize {
    fn from(value: RoomId) -> Self {
        Self::from(value.0)
    }
}
