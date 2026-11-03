use bevy::app::{App, Plugin, Startup};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, ResMut};
use campfire_combat::OnDeath;
use campfire_declarations::ById;
use campfire_sim::MatchRate;
use campfire_values::UnitType;

use crate::view::effects::MatchEffects;

/// Combat's part of the effects: the ticks each type keeps its dead, at whose half a death's
/// middle plays, given each type's death plays as the effects load, so the play of a death names
/// no book of combat's (02-engine-core.md, A client module list; 02-engine-core.md, A death plays
/// its phases).
#[derive(Debug)]
pub(crate) struct DeathKeepsPlugin;

impl Plugin for DeathKeepsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::keep.after(MatchEffects::load));
    }
}

impl DeathKeepsPlugin {
    /// Gives each type's death plays, of a client that loads effects, the ticks its type keeps
    /// its dead, as its `OnDeath` says at the match's rate.
    fn keep(
        effects: Option<ResMut<'_, MatchEffects>>,
        (on_death, rate): (Res<'_, ById<UnitType, OnDeath>>, Res<'_, MatchRate>),
    ) {
        let Some(mut effects) = effects else {
            return;
        };
        for (&unit_type, plays) in &mut effects.plays.deaths {
            plays.kept = on_death
                .get(unit_type)
                .and_then(|&on_death| on_death.kept(*rate));
        }
    }
}
