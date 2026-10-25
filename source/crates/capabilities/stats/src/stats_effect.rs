use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Ticks};
use campfire_math::Num;
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::StableId;
use campfire_units::PoolId;
use campfire_values::{ModifierId, Tag};

use crate::Stats;
use crate::applier::Applier;
use crate::modifier_adds::{HeldModifierAdds, ModifierAdd};
use crate::modifier_handle::ModifierHandle;
use crate::stats_call::StatsCall;
use crate::stats_world::StatsWorld;

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
    /// The end of `player`'s modifier `id`, if it holds it.
    RemovePlayer { player: PlayerSlot, id: ModifierId },
    /// The end of the instance of `id` from `source` on `carrier`.
    Remove {
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    },
    /// The end of the applications of `carrier`'s modifiers that grant `tag`, whatever their
    /// source; an instance a hold keeps stays.
    Purge { carrier: StableId, tag: Tag },
    /// `amount` of `unit`'s `pool`, unscaled, when it lives.
    Restore {
        unit: StableId,
        pool: PoolId,
        amount: Num,
    },
}

impl StatsEffect {
    /// `ctx.remove(m)`: the effect that ends the instance of `handle`, which the call now sees
    /// as gone.
    pub(crate) fn removing(handle: &ModifierHandle) -> Self {
        let mut data = handle.data();
        data.removed = true;
        Self::Remove {
            carrier: data.carrier,
            id: data.id,
            source: data.source,
        }
    }

    /// Applies the effect by `applier`, from a world alone, its added modifier's shares and state
    /// held first, as a module that is no call applies one.
    pub(crate) fn apply_world(self, world: &mut World, applier: Applier) {
        self.apply_by(world, applier, None);
    }

    /// Applies the effect, which a call by `applier` queued, an added modifier as
    /// `HeldModifierAdds::add` adds it: through `adds`, the call frame's, or with none, from a
    /// world alone.
    fn apply_by(self, world: &mut World, applier: Applier, adds: Option<&mut HeldModifierAdds>) {
        match self {
            Self::Add {
                target,
                id,
                duration,
            } => {
                let add = ModifierAdd {
                    target,
                    id,
                    applier,
                    duration,
                };
                match adds {
                    Some(adds) => adds.add(world, add),
                    None => Stats::add_modifier(world, add),
                }
            }
            Self::AddPlayer { player, id } => {
                world
                    .player_modifiers_mut(player)
                    .expect("a player of the session holds its modifiers")
                    .add(id);
            }
            Self::RemovePlayer { player, id } => {
                world
                    .player_modifiers_mut(player)
                    .expect("a player of the session holds its modifiers")
                    .remove(id);
            }
            Self::Remove {
                carrier,
                id,
                source,
            } => Stats::remove_modifier(world, carrier, id, source),
            Self::Purge { carrier, tag } => Stats::purge(world, carrier, tag),
            Self::Restore { unit, pool, amount } => Stats::restore(world, unit, pool, amount),
        }
    }
}

/// From the call's acting unit, by its action at its rank.
impl Effect for StatsEffect {
    const KIND: EffectKind = EffectKind::Change;

    fn apply(self, world: &mut World, frame: &mut Frame) {
        let applier = Applier {
            source: frame.acting(),
            action: frame.action(),
            rank: frame.rank(),
            hold: None,
        };
        self.apply_by(world, applier, Some(StatsCall::of_mut(frame).adds()));
    }
}
