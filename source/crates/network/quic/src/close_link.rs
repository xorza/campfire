use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;

use crate::app_code::AppCode;

/// Ends the link of `entity` and says why: its connection closes with `code`, which the peer reads
/// as `LinkEnd::ClosedByPeer`, and Lightyear unlinks it (11-quic-transport.md, A link's end is a
/// type). A link with no QUIC connection, as an in-process one, only unlinks.
#[derive(EntityEvent, Debug, Clone, Copy)]
pub struct CloseLink {
    pub entity: Entity,
    pub code: AppCode,
}
