use std::net::SocketAddr;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_net_link::{LinkOf, RemoteAddress};

/// How an `InProcessMatch` lays the server's links out in its tables. A query gives entities in
/// the order of their tables, which the stack's parallel commands change from run to run once
/// Bevy's `multi_threaded` is on, so a match plays the same in both layouts when the server takes
/// nothing in a query's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkLayout {
    /// The clients connect in player order, and the links lie as Bevy puts them.
    InOrder,
    /// The clients connect in reverse, and before each server frame the links' rows go in the
    /// reverse of player order. The players take the same slots: one frame's joins go in the order
    /// of their main keys, and each player's sorts after the one before.
    Reversed,
}

/// The address of each client's link, in client order, by which the server of a reversed layout
/// puts its links' rows in that order.
#[derive(Resource, Debug, Default)]
pub(crate) struct LinkRows {
    pub(crate) addresses: Vec<SocketAddr>,
    /// The server's links `lay_out` moves, in client order.
    moved: Vec<Entity>,
}

/// A mark that moves a server link out of its table and back, so its row goes last.
#[derive(Component, Debug)]
struct Moved;

impl LinkRows {
    /// Puts the rows of the server's links in each of their tables in client order: all go out to
    /// another table, then back in client order, as Bevy appends a row that enters a table.
    pub(crate) fn lay_out(world: &mut World) {
        world.resource_scope(|world, mut rows: Mut<'_, Self>| {
            let Self { addresses, moved } = &mut *rows;
            moved.clear();
            let mut links = world.query::<(Entity, &RemoteAddress)>();
            for address in &*addresses {
                let link = links
                    .iter(world)
                    .find(|(_, remote)| remote.0 == *address)
                    .map(|(link, _)| link);
                moved.extend(link.filter(|&link| world.get::<LinkOf>(link).is_some()));
            }
            for &link in &*moved {
                world.entity_mut(link).insert(Moved);
            }
            for &link in &*moved {
                world.entity_mut(link).remove::<Moved>();
            }
        });
    }
}
