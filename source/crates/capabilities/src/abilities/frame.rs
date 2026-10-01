use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{EntityIndex, StableId};

use crate::abilities::ability_book::AbilityId;
use crate::combat::Combat;
use crate::combat::attack_kind::AttackKind;
use crate::combat::attack_stats::AttackStats;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_kind::DamageKind;
use crate::combat::damage_queue::DamageQueue;
use crate::scripts::error::CallError;
use crate::stats::Stats;
use crate::stats::modifier_book::{Applier, ModifierId};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::values::name_table::NameTable;
use crate::values::param::Param;
use crate::values::scalar::Scalar;

/// What `on_cast` calls and modifier hooks read and queue, beside the units the view holds. One
/// frame serves the whole match, its buffers cleared and filled again, so a call allocates none
/// of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's params, one run per ability, by ability id; and every loaded
    /// modifier's, by modifier id.
    params: NameTable<Param>,
    modifier_params: NameTable<Param>,
    /// The running call's ability, its rank, and its acting unit: a cast's caster, or a hook's
    /// modifier source; and the hook's modifier.
    cast: Option<AbilityId>,
    pub(crate) rank: u8,
    pub(crate) caster: Option<StableId>,
    modifier: Option<ModifierId>,
    /// The depth of the chain of events the running call is in: 0 for a cast.
    depth: u8,
    /// The running call's params at its rank, in the order of their names: its ability's, and
    /// its modifier's.
    values: Vec<Scalar>,
    modifier_values: Vec<Scalar>,
    /// The effects the running cast queued, in order, and the modifier handles it took.
    pub(crate) effects: Vec<Effect>,
    pub(crate) handles: Vec<ModifierHandle>,
}

/// An effect a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Damage {
        target: StableId,
        amount: Num,
        kind: DamageKind,
    },
    Heal {
        unit: StableId,
        amount: Num,
    },
    Restore {
        unit: StableId,
        amount: Num,
    },
    Modifier(ModifierEffect),
    AttackHit {
        target: StableId,
    },
}

impl Frame {
    /// Adds the params of `ability`, the one the book loads next.
    pub(crate) fn add_params(&mut self, ability: AbilityId, params: &BTreeMap<String, Param>) {
        let run = self.params.push(
            params
                .iter()
                .map(|(name, param)| (name.as_str(), param.clone())),
        );
        debug_assert_eq!(run, ability.index(), "one run of params per ability");
    }

    /// Adds the params of `modifier`, the one the book loaded last.
    pub(crate) fn add_modifier_params(
        &mut self,
        modifier: ModifierId,
        params: &BTreeMap<String, Param>,
    ) {
        let run = self.modifier_params.push(
            params
                .iter()
                .map(|(name, param)| (name.as_str(), param.clone())),
        );
        debug_assert_eq!(run, modifier.index(), "one run of params per modifier");
    }

    /// Starts a cast of `ability` at `rank`, with its params at that rank; a param that
    /// overflows there fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        ability: AbilityId,
        rank: u8,
        caster: StableId,
    ) -> Result<(), CallError> {
        self.begin(Some(ability), rank, Some(caster), None, 0)
    }

    /// Starts a hook of `modifier` at chain depth `depth`, whose instance came from `source`
    /// by `ability` at `rank`: the modifier's params, then the ability's, at that rank; a param
    /// that overflows there fails the call.
    pub(crate) fn begin_hook(
        &mut self,
        modifier: ModifierId,
        ability: Option<AbilityId>,
        rank: u8,
        source: Option<StableId>,
        depth: u8,
    ) -> Result<(), CallError> {
        self.begin(ability, rank, source, Some(modifier), depth)
    }

    fn begin(
        &mut self,
        ability: Option<AbilityId>,
        rank: u8,
        acting: Option<StableId>,
        modifier: Option<ModifierId>,
        depth: u8,
    ) -> Result<(), CallError> {
        self.cast = ability;
        self.rank = rank;
        self.caster = acting;
        self.modifier = modifier;
        self.depth = depth;
        self.effects.clear();
        self.handles.clear();
        let fill = |values: &mut Vec<Scalar>, params: &[Param]| {
            values.clear();
            values.reserve_exact(params.len());
            for param in params {
                values.push(param.at(rank).ok_or(CallError::ParamOverflow)?);
            }
            Ok(())
        };
        let params = ability.map_or(&[][..], |ability| self.params.values(ability.index()));
        fill(&mut self.values, params)?;
        let params = modifier.map_or(&[][..], |modifier| {
            self.modifier_params.values(modifier.index())
        });
        fill(&mut self.modifier_values, params)
    }

    /// Applies the effects the call that ran queued, in order, from its acting unit and its
    /// ability at its rank: damage joins the queue at the call's depth, heals, restores and
    /// modifiers apply at once. Then what it wrote to modifier handles applies.
    pub(crate) fn apply(&mut self, world: &mut World) {
        let (source, ability, rank) = (self.caster, self.cast, self.rank);
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            ability,
            depth: self.depth,
        };
        for &effect in &self.effects {
            match effect {
                Effect::Damage {
                    target,
                    amount,
                    kind,
                } => {
                    let damage = damage(target, amount, kind, DamageCause::Effect);
                    world.resource_mut::<DamageQueue>().push(damage);
                }
                Effect::Heal { unit, amount } => Combat::heal(world, unit, amount),
                Effect::Restore { unit, amount } => Combat::restore(world, unit, amount),
                Effect::Modifier(effect) => {
                    let applier = Applier {
                        source,
                        ability,
                        rank,
                        passive: false,
                        aura: false,
                    };
                    let param = |name: &str| self.ability_param(ability?, rank, name);
                    Stats::apply_effect(world, effect, applier, param);
                }
                Effect::AttackHit { target } => {
                    let index = world.resource::<EntityIndex>();
                    let entity = source.and_then(|source| index.get(source));
                    let Some(stats) = entity.and_then(|entity| world.get::<AttackStats>(entity))
                    else {
                        continue;
                    };
                    let kind = world.resource::<AttackKind>().0;
                    let hit = damage(target, stats.damage(), kind, DamageCause::ExtraAttack);
                    world.resource_mut::<DamageQueue>().push(hit);
                }
            }
        }
        self.effects.clear();
        for handle in self.handles.drain(..) {
            Stats::write_handle(world, &handle);
        }
    }

    /// Param `name` of `ability` at `rank`, if it declares one and it resolves there.
    pub(crate) fn ability_param(&self, ability: AbilityId, rank: u8, name: &str) -> Option<Scalar> {
        let at = self.params.find(ability.index(), name)?;
        self.params.values(ability.index())[at].at(rank)
    }

    /// The running call's param `name`: its modifier's, then its ability's, if either declares
    /// one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let own = self.modifier.and_then(|modifier| {
            let at = self.modifier_params.find(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        own.or_else(|| {
            let at = self.params.find(self.cast?.index(), name)?;
            Some(self.values[at])
        })
    }
}
