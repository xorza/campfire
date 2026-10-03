use campfire_script::ScriptId;

use crate::actions::action_data::RankToggle;
use crate::actions::delivery::Delivery;
use crate::actions::kind_spec::KindSpec;
use crate::actions::rank_values::{ChannelRule, ChargeRule, RankValues};
use crate::players::resource_amount::ResourceAmount;
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::tag_set::TagSet;
use crate::values::attitude::Attitude;

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
    pub(crate) fn has_rank(&self, rank: u8) -> bool {
        rank > 0 && usize::from(rank) <= self.ranks.len()
    }

    /// Whether a slot may hold it at `rank`: one of its ranks, or 0 before it is learned.
    pub(crate) fn slots_at(&self, rank: u8) -> bool {
        usize::from(rank) <= self.ranks.len()
    }

    /// Its charges at `rank`; none at a rank it does not have, as 0 before it is learned.
    pub(crate) fn charge_rule(&self, rank: u8) -> Option<ChargeRule> {
        self.has_rank(rank)
            .then(|| self.values(rank).charges)
            .flatten()
    }

    /// Its channel at `rank`; none at a rank it does not have, as 0 before it is learned.
    pub(crate) fn channel_rule(&self, rank: u8) -> Option<ChannelRule> {
        self.has_rank(rank)
            .then(|| self.values(rank).channel)
            .flatten()
    }

    /// Its toggle at `rank`; none at a rank it does not have, as 0 before it is learned.
    pub(crate) fn toggle_rule(&self, rank: u8) -> Option<RankToggle> {
        self.has_rank(rank)
            .then(|| self.values(rank).toggle)
            .flatten()
    }

    /// Its capability fields at `rank`.
    pub(crate) fn values(&self, rank: u8) -> RankValues {
        self.ranks[usize::from(rank - 1)]
    }

    /// Its cost at `rank` in its caster's player's resources.
    pub(crate) fn resource_cost(&self, rank: u8) -> &[ResourceAmount] {
        let per_rank = self.resource_costs.len() / self.ranks.len().max(1);
        let start = usize::from(rank - 1) * per_rank;
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
    /// against a unit of `tags` it regards with `attitude`, or with `None`, against any: the one
    /// rule of `ActionBook::weapon_for` and of the script view.
    pub(crate) fn arms(
        rank: u8,
        weapon: Option<Filter>,
        target: Option<(Attitude, TagSet)>,
    ) -> bool {
        rank > 0
            && weapon.is_some_and(|filter| {
                target.is_none_or(|(attitude, tags)| filter.selects(attitude, tags))
            })
    }
}
