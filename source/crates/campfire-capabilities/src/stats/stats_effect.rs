use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_sim::StableId;

use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::stats::Stats;
use crate::stats::applier::Applier;
use crate::stats::player_modifiers::{PlayerModifier, PlayerModifiers};
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;

/// A change to a unit's modifiers that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatsEffect {
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
                let held = PlayerModifier {
                    player,
                    modifier: id,
                };
                world.resource_mut::<PlayerModifiers>().add(held);
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
    fn apply(self, world: &mut World, frame: &mut Frame, _: Tick) {
        let applier = Applier {
            source: frame.acting(),
            ability: frame.action(),
            rank: frame.rank(),
            hold: None,
        };
        self.apply_by(world, applier);
    }
}
