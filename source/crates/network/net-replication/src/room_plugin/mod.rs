/*! Room-based network relevance module, where you can use semi-static rooms to manage network relevance

# Room

Rooms are used to provide interest management in a semi-static way.
Entities and Clients can be added to multiple rooms.

If an entity and a client are in the same room, then the entity will be relevant to the client.
If an entity leaves a room that a client is in, or if a client leaves a room that an entity is in,
then the entity won't be relevant to that client (and will despawned for that client)

## Example

This can be useful for games where you have physical instances of rooms:
- a RPG where you can have different rooms (tavern, cave, city, etc.)
- a server could have multiple lobbies, and each lobby is in its own room
- a map could be divided into a grid of 2D squares, where each square is its own room

```rust
# use bevy_app::App;
# use campfire_net_replication::{Replicate, ReplicationSender, RoomAllocator, RoomPlugin, Rooms};

# let mut app = App::new();
# app.add_plugins(RoomPlugin);
# let room = app.world_mut().resource_mut::<RoomAllocator>().allocate();
# let mut commands = app.world_mut().commands();
// Add the client and entity to the same room: the entity will be
// replicated/visible to clients sharing that room.
let entity = commands.spawn((Replicate::default(), Rooms::single(room))).id();
let client = commands.spawn((ReplicationSender::default(), Rooms::single(room))).id();
```

*/
use bevy_app::{App, Plugin};
use bevy_replicon::prelude::AppVisibilityExt;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;

use crate::room_allocator::RoomAllocator;
use crate::rooms::Rooms;

/// Interest by [`Rooms`]: a hierarchy member follows its root's rooms unless it holds
/// [`Overridden<Rooms>`](crate::overridden::Overridden) (11-net-stack/replication.md, A member
/// follows its root for each aspect, unless it overrides that aspect).
#[derive(Debug, Default)]
pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FilterRegistry>()
            .init_resource::<ReplicationRegistry>()
            .init_resource::<RoomAllocator>();
        app.add_visibility_filter::<Rooms>();
    }
}

#[cfg(test)]
mod tests;
