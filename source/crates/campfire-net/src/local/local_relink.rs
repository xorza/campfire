use bevy_app::{App, Plugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Res, Single};
use lightyear::prelude::{Client, Connected};

use crate::local::local_server::Relinks;

/// Gives a local server's client the new link the server makes once it starts again after a
/// load: the client's link failed as the server's app ended, and it connects through the new one
/// once its wait passed. The link waits until the client has been without one for a whole frame,
/// so replication's client state passes through its disconnected state, which clears what the
/// old server sent: the new server counts its ticks from 0 again, and a tick of the old server's
/// left in that state would be taken for one of the new server's.
#[derive(Debug)]
pub struct LocalRelink {
    pub relinks: Relinks,
}

/// The links a local server makes after its loads.
#[derive(Resource, Debug)]
struct Relinked(Relinks);

impl Plugin for LocalRelink {
    fn build(&self, app: &mut App) {
        app.insert_resource(Relinked(self.relinks.clone()));
        app.add_systems(Update, LocalRelink::take);
    }
}

impl LocalRelink {
    fn take(
        relinked: Res<'_, Relinked>,
        client: Single<'_, '_, (Entity, Has<Connected>), With<Client>>,
        mut unlinked_since: Local<'_, bool>,
        mut commands: Commands<'_, '_>,
    ) {
        let (client, connected) = *client;
        if connected {
            *unlinked_since = false;
            return;
        }
        if !*unlinked_since {
            *unlinked_since = true;
            return;
        }
        if let Some(link) = relinked.0.take() {
            commands.entity(client).insert(link);
        }
    }
}
