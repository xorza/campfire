use bevy_ecs::{component::Component, reflect::ReflectComponent};
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A received entity the client predicts, by the sender's `PredictionTarget` or by the receiver's
/// own insert; it is the replication crate's, as prediction and replication both name it.
#[derive(Component, Clone, Copy, Debug, Default, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Predicted;
