use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Ticks};
use campfire_script::{Effect, Frame};
use campfire_sim::StableId;
use campfire_values::{ModifierId, Tag};

use crate::Stats;
use crate::applier::Applier;
use crate::stats_world::StatsWorldCrate;

/// A change to a unit's modifiers that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsEffect {
    /// Modifier `id` on `target`, for `duration` when the call names one.
    Add {
        target: StableId,
        id: ModifierId,
        duration: Option<Ticks>,
    },
    /// Modifier `id` held by `player`, for the units it owns.
    AddPlayer { player: PlayerSlot, id: ModifierId },
    /// The end of the instance of `id` from `source` on `carrier`.
    Remove {
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    },
    /// The end of the applications of `carrier`'s modifiers that grant `tag`, whatever their
    /// source; an instance a hold keeps stays.
    Purge { carrier: StableId, tag: Tag },
}

/// From the call's acting unit, by its action at its rank.
impl StatsEffect {
    /// Applies the effect, which a call by `applier` queued. An added modifier's numbers resolve
    /// now, its params read from the param book, of its source as it is now; nothing is added to a
    /// dead or gone unit, or one that carries no modifiers.
    pub(crate) fn apply_by(self, world: &mut World, applier: Applier) {
        match self {
            StatsEffect::Add {
                target,
                id,
                duration,
            } => Stats::add_modifier(world, target, id, applier, duration),
            StatsEffect::AddPlayer { player, id } => {
                world
                    .player_modifiers_mut(player)
                    .expect("a player of the session holds its modifiers")
                    .add(id);
            }
            StatsEffect::Remove {
                carrier,
                id,
                source,
            } => Stats::remove_modifier(world, carrier, id, source),
            StatsEffect::Purge { carrier, tag } => Stats::purge(world, carrier, tag),
        }
    }
}

impl Effect for StatsEffect {
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let applier = Applier {
            source: frame.acting(),
            action: frame.action(),
            rank: frame.rank(),
            hold: None,
        };
        self.apply_by(world, applier);
    }
}
