use bevy::prelude::*;
use bevy_replicon::prelude::Remote;
use campfire_net_connection::NetworkTarget;
use campfire_net_replication::Replicate;
use std::collections::BTreeMap;

use campfire_net_replication::internals::protocol::{CompA, CompS};

/// The server's side of a scenario's replication: eight entities at the first frame it runs,
/// one of them updated every ten frames, and two despawned at frame 100, while `running`.
#[derive(Resource, Debug)]
pub(crate) struct ReplicationWorkload {
    pub(crate) running: bool,
    frame: u32,
    entities: Vec<Entity>,
}

impl ReplicationWorkload {
    const ENTITIES: usize = 8;

    /// A workload that spawns its entities on its first frame.
    pub(crate) const fn running() -> Self {
        Self {
            running: true,
            frame: 0,
            entities: Vec::new(),
        }
    }

    pub(crate) fn step(
        mut workload: ResMut<'_, Self>,
        mut values: Query<'_, '_, &mut CompA>,
        mut commands: Commands<'_, '_>,
    ) {
        if !workload.running {
            return;
        }
        if workload.entities.is_empty() {
            workload.entities = (0..Self::ENTITIES)
                .map(|i| {
                    commands
                        .spawn((
                            CompS(format!("e{i}")),
                            CompA(0.0),
                            Replicate::to_clients(NetworkTarget::All),
                        ))
                        .id()
                })
                .collect();
        }
        workload.frame += 1;
        let frame = workload.frame;
        if frame.is_multiple_of(10) {
            let entity = workload.entities[usize::try_from(frame / 10).unwrap() % Self::ENTITIES];
            if let Ok(mut value) = values.get_mut(entity) {
                value.0 = frame as f32;
            }
        }
        if frame == 100 {
            for &entity in &workload.entities[..2] {
                commands.entity(entity).despawn();
            }
        }
    }

    /// Each replicated entity's name and value on the server.
    pub(crate) fn server_state(world: &mut World) -> BTreeMap<String, f32> {
        world
            .query_filtered::<(&CompS, &CompA), With<Replicate>>()
            .iter(world)
            .map(|(name, value)| (name.0.clone(), value.0))
            .collect()
    }

    /// Each replicated entity's name and value on a client, where replicon marks it `Remote`.
    pub(crate) fn client_state(world: &mut World) -> BTreeMap<String, f32> {
        world
            .query_filtered::<(&CompS, &CompA), With<Remote>>()
            .iter(world)
            .map(|(name, value)| (name.0.clone(), value.0))
            .collect()
    }
}
