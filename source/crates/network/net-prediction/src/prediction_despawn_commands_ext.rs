use bevy_ecs::prelude::*;

use crate::prediction_despawn_command::PredictionDespawnCommand;

pub trait PredictionDespawnCommandsExt {
    fn prediction_despawn(&mut self);
}

impl PredictionDespawnCommandsExt for EntityCommands<'_> {
    fn prediction_despawn(&mut self) {
        let entity = self.id();
        self.commands().queue(PredictionDespawnCommand { entity });
    }
}
