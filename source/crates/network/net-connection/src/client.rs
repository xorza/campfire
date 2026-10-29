use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;

use crate::connection_state::{Connected, Connecting, Disconnected, Disconnecting};

/// A client, which starts disconnected unless it comes with a connection state.
#[derive(Debug, Component, Default, Reflect)]
#[component(on_add = Client::on_add)]
pub struct Client;

impl Client {
    /// A hook and not a required `Disconnected`: a required state joins the state a spawn carries,
    /// and each state's hook then removes the other, which leaves none.
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = world.entity(context.entity);
        if !(entity.contains::<Connected>()
            || entity.contains::<Connecting>()
            || entity.contains::<Disconnecting>()
            || entity.contains::<Disconnected>())
        {
            world
                .commands()
                .entity(context.entity)
                .try_insert(Disconnected::default());
        }
    }
}
