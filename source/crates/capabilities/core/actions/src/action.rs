use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_script::{Filter, Hook, HookSet, ScriptId};
use campfire_sim::StableId;
use campfire_values::{ModifierId, PackageIndex, Rank, Relation, TagSet};

use crate::action_kind::ActionKind;
use crate::action_target::ActionTarget;
use crate::channel_pay::ChannelPay;
use crate::delivery::Delivery;
use crate::rank_toggle::TogglePer;
use crate::rank_values::RankValues;

/// An action as a match runs it.
#[derive(Debug, Clone)]
pub struct Action {
    /// Its package: `PackageIndex::MODE` the mode, then each package the mode depends on.
    pub package: PackageIndex,
    pub kind: ActionKind,
    /// The modifier its unit holds while it has a rank, and whether only while it is ready.
    pub passive: Option<Passive>,
    /// The modifier its unit holds while its toggle is on or its channel runs.
    pub hold: Option<ModifierId>,
    pub aim: Aim,
    /// The tags its unit must pass for it to start, for its passive to hold, and for an attack to
    /// be chosen as a weapon.
    pub when: Option<Filter>,
    /// Its capability fields at each rank, from rank 1.
    pub ranks: Vec<RankValues>,
    /// Its costs in its caster's player's resources, each rank's run where its values say.
    pub resource_costs: Box<[ResourceAmount]>,
    /// Its script, if it has one, and the action hooks the script defines: a script may serve
    /// only the action's modifiers.
    pub script: Option<ScriptId>,
    pub hooks: HookSet,
    /// How it delivers, other than at once.
    pub delivery: Option<Delivery>,
    /// What each tick of its channel pays its unit's owner.
    pub pays: Option<ChannelPay>,
}

/// An action's passive modifier, and whether its unit holds it only while the action is off
/// cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passive {
    pub modifier: ModifierId,
    pub while_ready: bool,
}

/// What an action aims at, as a match runs it: a unit target's filter resolved against the
/// match's unit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    None,
    /// A point, moved in to the action's range when it lies beyond it, with `clamp`.
    Point {
        clamp: bool,
    },
    Direction,
    Unit(Filter),
}

impl Action {
    /// Its script, when it defines `hook`.
    pub fn hook(&self, hook: Hook) -> Option<ScriptId> {
        self.script.filter(|_| self.hooks.contains(hook))
    }

    /// Whether a rank of it toggles.
    pub fn toggles(&self) -> bool {
        self.ranks.iter().any(|values| values.toggle.is_some())
    }

    /// Whether a rank of it toggles, paying its cost `per`.
    pub fn toggles_per(&self, per: TogglePer) -> bool {
        let mut toggles = self.ranks.iter().filter_map(|values| values.toggle);
        toggles.any(|toggle| toggle.per == per)
    }

    /// Whether a rank of it channels.
    pub fn channels(&self) -> bool {
        self.ranks.iter().any(|values| values.channel.is_some())
    }

    /// Whether `rank` is one of its ranks, from 1 to its last.
    pub const fn has_rank(&self, rank: Rank) -> bool {
        rank.index() < self.ranks.len()
    }

    /// Whether a slot may hold it at `rank`: one of its ranks, or none before it is learned.
    pub(crate) fn slots_at(&self, rank: Option<Rank>) -> bool {
        rank.is_none_or(|rank| self.has_rank(rank))
    }

    /// Its capability fields at `rank`; none at a rank it does not have.
    pub fn values_at(&self, rank: Rank) -> Option<&RankValues> {
        self.ranks.get(rank.index())
    }

    /// Its capability fields at `rank`, one of its ranks.
    pub fn values(&self, rank: Rank) -> &RankValues {
        self.values_at(rank).expect("a rank the action has")
    }

    /// Its windup at `rank` in ticks, as a `Num`: a build's time. None for a rank it lacks, or a
    /// count past a `Num`.
    pub fn windup_ticks(&self, rank: Rank) -> Option<Num> {
        let ticks = self.values_at(rank)?.windup.get();
        Num::from_int(i64::try_from(ticks).ok()?)
    }

    /// Its cost at `rank`, one of its ranks, in its caster's player's resources.
    pub fn resource_cost(&self, rank: Rank) -> &[ResourceAmount] {
        &self.resource_costs[self.values(rank).resource_cost.range()]
    }

    /// The target a start of it takes of `target`, as `unit` reads a unit target: none for an
    /// action that aims at nothing, whatever it is given; a point for one that aims at a point or
    /// along a direction; a unit for one that aims at a unit, when `unit` gives it as living, with
    /// how the caster regards it and its tags, and the action's filter selects it. `None` when
    /// it takes no such target. The one rule of a start's target, which a search for one runs too
    /// (04-capabilities/control.md, A bot's search runs its order's own test).
    pub fn start_target(
        &self,
        target: ActionTarget,
        unit: impl FnOnce(StableId) -> Option<(Relation, TagSet)>,
    ) -> Option<ActionTarget> {
        match (self.aim, target) {
            (Aim::None, _) => Some(ActionTarget::None),
            (Aim::Point { .. } | Aim::Direction, ActionTarget::Point(at)) => {
                Some(ActionTarget::Point(at))
            }
            (Aim::Unit(filter), ActionTarget::Unit(id)) => unit(id)
                .is_some_and(|(relation, tags)| filter.selects(relation, tags))
                .then_some(ActionTarget::Unit(id)),
            _ => None,
        }
    }

    /// The filter of the units it may attack, for a weapon, which the load lets aim only at a
    /// unit; `None` for another kind.
    pub(crate) const fn weapon_filter(&self) -> Option<Filter> {
        match (self.kind, self.aim) {
            (ActionKind::Attack, Aim::Unit(filter)) => Some(filter),
            _ => None,
        }
    }

    /// Whether a unit of `holder`'s tags passes its `when` (upgrades.md, An action may need tags).
    pub fn usable_by(&self, holder: TagSet) -> bool {
        self.when.is_none_or(|when| when.selects_tags(holder))
    }

    /// Whether a slot at `rank` of this action arms its unit, of `holder`'s tags, against a unit
    /// of `tags` it regards with `relation`, or with `None`, against any: an attack whose `when`
    /// its unit passes and whose filter selects the target; the one rule of
    /// `ActionSlots::weapon_in` and of the script view.
    pub(crate) fn arms(
        &self,
        rank: Option<Rank>,
        holder: TagSet,
        target: Option<(Relation, TagSet)>,
    ) -> bool {
        rank.is_some()
            && self.usable_by(holder)
            && self.weapon_filter().is_some_and(|filter| {
                target.is_none_or(|(relation, tags)| filter.selects(relation, tags))
            })
    }
}
