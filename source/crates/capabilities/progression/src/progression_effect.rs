use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::{Effect, EffectKind, Frame, View};
use campfire_sim::{SimWorld, StableId};
use campfire_values::TrackId;

use crate::error::ProgressionRefusal;
use crate::level_ups::LevelUps;
use crate::progression_column::ProgressionColumn;

/// A change to units' progress that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressionEffect {
    /// Experience, not negative, on a track the unit has.
    AddXp {
        id: StableId,
        track: TrackId,
        amount: Num,
    },
}

impl ProgressionEffect {
    /// Experience of `amount` to `id` on `track`, which `ctx.add_xp` and a listed `xp` queue
    /// alike: refused when it is negative, or when `view` holds no unit `id` with the track.
    pub(crate) fn add_xp(
        view: &View,
        id: StableId,
        track: TrackId,
        amount: Num,
    ) -> Result<Self, ProgressionRefusal> {
        if amount < Num::ZERO {
            return Err(ProgressionRefusal::NegativeXp);
        }
        if !ProgressionColumn::has(view, id, track) {
            return Err(ProgressionRefusal::NoTrack);
        }
        Ok(Self::AddXp { id, track, amount })
    }
}

impl Effect for ProgressionEffect {
    const KIND: EffectKind = EffectKind::Change;

    /// Applies the effect: experience raises its track's level, and each level reached joins the
    /// tick's level-ups. A level reached on the `level` track gives the unit a point for each
    /// level, and raises the unit's level, which stats owns.
    fn apply(self, world: &mut World, _: &mut Frame) {
        match self {
            Self::AddXp { id, track, amount } => {
                let entity = world.entity_of(id).expect("a unit given experience exists");
                LevelUps::grant(world, entity, id, track, amount);
            }
        }
    }
}
