use bevy_app::{App, Plugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Res, Single};
use lightyear::prelude::Client;

use crate::local_server::Relinks;

/// Gives a local server's client the new link the server makes once it starts again after a
/// load: the client's link failed as the server's app ended, and it connects through the new one
/// once its wait passed.
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
        client: Single<'_, '_, Entity, With<Client>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Some(link) = relinked.0.take() {
            commands.entity(*client).insert(link);
        }
    }
}
