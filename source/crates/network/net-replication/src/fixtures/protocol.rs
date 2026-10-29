use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;
use bevy_ecs::entity::{Entity, MapEntities};
use bevy_ecs::error::Result;
use bevy_reflect::Reflect;
use bevy_replicon::shared::replication::diff::Diffable;
use serde::{Deserialize, Serialize};

use crate::app_component_ext::AppComponentExt;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompA(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
pub struct CompS(pub String);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompReplicateOnce(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect, MapEntities)]
pub struct CompMap(#[entities] pub Entity);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompSimple(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
pub struct CompRepliconDiff(pub u32);

impl Diffable for CompRepliconDiff {
    type Diff = u32;

    fn apply_diff(&mut self, diff: &Self::Diff) -> Result<()> {
        self.0 = *diff;
        Ok(())
    }
}

/// The replication layer's test protocol: one component of each replication rule.
#[derive(Debug)]
pub struct ReplicationProtocolPlugin;

impl Plugin for ReplicationProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<CompA>().replicate();
        app.register_component::<CompS>().replicate();
        app.register_component::<CompReplicateOnce>()
            .replicate_once();
        app.register_component::<CompSimple>().replicate();
        app.register_component::<CompMap>().replicate();
        app.register_component::<CompRepliconDiff>()
            .replicate_diff();
    }
}
