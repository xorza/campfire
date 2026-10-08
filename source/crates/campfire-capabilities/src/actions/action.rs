use campfire_math::Num;
use campfire_script::ScriptId;

use crate::actions::delivery::Delivery;
use crate::actions::kind_spec::KindSpec;
use crate::actions::rank_values::RankValues;
use crate::players::resource_amount::ResourceAmount;
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::tag_set::TagSet;
use crate::values::rank::Rank;
use crate::values::relation::Relation;

/// An action as a match runs it.
#[derive(Debug, Clone)]
pub(crate) struct Action {
    /// Its package: 0 the mode, then each package the mode depends on, and its name there.
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) kind: KindSpec,
    /// The modifier its unit holds while it has a rank, and whether only while it is ready.
    pub(crate) passive: Option<Passive>,
    /// The modifier its unit holds while its toggle is on or its channel runs.
    pub(crate) hold: Option<ModifierId>,
    pub(crate) aim: Aim,
    /// Its capability fields at each rank, from rank 1.
    pub(crate) ranks: Vec<RankValues>,
    /// Its cost in its caster's player's resources at each rank, one run of the same resources
    /// each, rank after rank.
    pub(super) resource_costs: Box<[ResourceAmount]>,
    /// Its script, if it has one, and the action hooks the script defines: a script may serve
    /// only the action's modifiers.
    pub(super) script: Option<ScriptId>,
    pub(super) hooks: HookSet,
    /// How it delivers, other than at once.
    pub(crate) delivery: Option<Delivery>,
}

/// An action's passive modifier, and whether its unit holds it only while the action is off
/// cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Passive {
    pub(crate) modifier: ModifierId,
    pub(crate) while_ready: bool,
}

/// What an action aims at, as a match runs it: a unit target's filter resolved against the
/// match's unit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
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
    pub(crate) fn hook(&self, hook: Hook) -> Option<ScriptId> {
        self.script.filter(|_| self.hooks.contains(hook))
    }

    /// Whether `rank` is one of its ranks, from 1 to its last.
    pub(crate) fn has_rank(&self, rank: Rank) -> bool {
        rank.index() < self.ranks.len()
    }

    /// Whether a slot may hold it at `rank`: one of its ranks, or none before it is learned.
    pub(crate) fn slots_at(&self, rank: Option<Rank>) -> bool {
        rank.is_none_or(|rank| self.has_rank(rank))
    }

    /// Its capability fields at `rank`; none at a rank it does not have.
    pub(crate) fn values_at(&self, rank: Rank) -> Option<&RankValues> {
        self.ranks.get(rank.index())
    }

    /// Its capability fields at `rank`, one of its ranks.
    pub(crate) fn values(&self, rank: Rank) -> &RankValues {
        self.values_at(rank).expect("a rank the action has")
    }

    /// Its windup at `rank` in ticks, as a `Num`: a build's time. None for a rank it lacks, or a
    /// count past a `Num`.
    pub(crate) fn windup_ticks(&self, rank: Rank) -> Option<Num> {
        let ticks = self.values_at(rank)?.windup.get();
        Num::from_int(i64::try_from(ticks).ok()?)
    }

    /// Its cost at `rank`, one of its ranks, in its caster's player's resources.
    pub(crate) fn resource_cost(&self, rank: Rank) -> &[ResourceAmount] {
        let per_rank = self.resource_costs.len() / self.ranks.len().max(1);
        let start = rank.index() * per_rank;
        &self.resource_costs[start..start + per_rank]
    }

    /// The filter of the units it may attack, for a weapon, which the load lets aim only at a
    /// unit; `None` for another kind.
    pub(crate) const fn weapon_filter(&self) -> Option<Filter> {
        match (self.kind, self.aim) {
            (KindSpec::Attack(_), Aim::Unit(filter)) => Some(filter),
            _ => None,
        }
    }

    /// Whether a slot at `rank` whose action has the weapon filter `weapon` arms its unit
    /// against a unit of `tags` it regards with `relation`, or with `None`, against any: the one
    /// rule of `ActionBook::weapon_for` and of the script view.
    pub(crate) fn arms(
        rank: Option<Rank>,
        weapon: Option<Filter>,
        target: Option<(Relation, TagSet)>,
    ) -> bool {
        rank.is_some()
            && weapon.is_some_and(|filter| {
                target.is_none_or(|(relation, tags)| filter.selects(relation, tags))
            })
    }
}
