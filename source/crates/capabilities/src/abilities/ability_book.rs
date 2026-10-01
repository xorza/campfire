use bevy_ecs::resource::Resource;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{TickRate, Ticks};
use serde::{Deserialize, Serialize};

use crate::abilities::action_data::{ActionData, CostTarget, Range};
use crate::abilities::error::AbilityError;
use crate::mode::resource_id::ResourceAmount;
use crate::scripts::hook::Hook;
use crate::stats::modifier_book::ModifierId;
use crate::stats::pool_cost::PoolCost;
use crate::units::filter::Filter;
use crate::values::declared_name::DeclaredName;

/// The abilities a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does.
#[derive(Resource, Debug, Default)]
pub(crate) struct AbilityBook {
    abilities: Vec<Ability>,
}

/// An ability, by its place in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbilityId(u32);

/// An ability as a match runs it.
#[derive(Debug)]
pub(crate) struct Ability {
    /// Its package: 0 the mode, then each package the mode depends on.
    pub(crate) package: u16,
    /// The modifier its unit holds while it has a rank, and whether only while it is ready.
    pub(crate) passive: Option<Passive>,
    pub(crate) aim: Aim,
    /// Its capability fields at each rank, from rank 1.
    pub(crate) ranks: Vec<RankValues>,
    /// Its cost in its caster's player's resources at each rank, one run of the same resources
    /// each, rank after rank.
    resource_costs: Box<[ResourceAmount]>,
    /// The script, when it defines `on_resolve`: a script may serve only the ability's modifiers.
    pub(crate) on_resolve: Option<ScriptId>,
}

/// An ability's passive modifier, and whether its unit holds it only while the ability is off
/// cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Passive {
    pub(crate) modifier: ModifierId,
    pub(crate) while_ready: bool,
}

/// What an ability aims at, as a match runs it: a unit target's filter resolved against the
/// match's unit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
    None,
    Point,
    Direction,
    Unit(Filter),
}

/// An ability's fields at each rank: its values, and its cost in player resources, one run of
/// the same resources a rank.
#[derive(Debug)]
pub(crate) struct LoadedRanks {
    pub(crate) values: Vec<RankValues>,
    pub(crate) resource_costs: Vec<ResourceAmount>,
}

/// An ability's capability fields at one rank, times in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RankValues {
    pub(crate) range: Range,
    pub(crate) cooldown: Ticks,
    pub(crate) cost: PoolCost,
    pub(crate) cast_time: Ticks,
}

impl AbilityBook {
    /// Loads `data`, aiming at `aim`, with its fields at each rank `ranks` holds; see
    /// `Abilities::load`.
    #[expect(
        clippy::too_many_arguments,
        reason = "an ability's parts, each from its own place"
    )]
    pub(crate) fn load(
        &mut self,
        host: &ScriptHost,
        package: u16,
        passive: Option<Passive>,
        data: &ActionData,
        script: Option<ScriptId>,
        aim: Aim,
        ranks: LoadedRanks,
    ) -> AbilityId {
        assert_eq!(
            data.script.is_some(),
            script.is_some(),
            "an ability has a script exactly when its data names one"
        );
        let hook = Hook::OnResolve;
        let on_resolve = script.filter(|&script| host.defines(script, hook.name(), hook.params()));
        let id = AbilityId(u32::try_from(self.abilities.len()).expect("abilities fit u32"));
        self.abilities.push(Ability {
            package,
            passive,
            aim,
            ranks: ranks.values,
            resource_costs: ranks.resource_costs.into_boxed_slice(),
            on_resolve,
        });
        id
    }

    pub(crate) fn get(&self, id: AbilityId) -> Option<&Ability> {
        self.abilities.get(id.index())
    }
}

impl Ability {
    /// Its cost at `rank` in its caster's player's resources.
    pub(crate) fn resource_cost(&self, rank: u8) -> &[ResourceAmount] {
        let per_rank = self.resource_costs.len() / self.ranks.len().max(1);
        let start = usize::from(rank - 1) * per_rank;
        &self.resource_costs[start..start + per_rank]
    }
}

impl AbilityId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl RankValues {
    /// The fields of `data`, which the package load checked, at each of its `ranks` ranks,
    /// times in ticks at `rate`, what its cost's names take from by `target`; an error when a
    /// time does not count in ticks.
    pub(crate) fn all(
        data: &ActionData,
        ranks: u8,
        rate: TickRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<LoadedRanks, AbilityError> {
        assert!(
            data.check_ranks(usize::from(ranks)),
            "the load checked the ranks"
        );
        let ticks = |ms: u64| rate.ticks(ms).ok_or(AbilityError::TimeTooLarge);
        let mut loaded = LoadedRanks {
            values: Vec::with_capacity(usize::from(ranks)),
            resource_costs: Vec::new(),
        };
        for rank in 1..=ranks {
            let fields = data
                .fields_at(rank, &target)
                .expect("the load checked the fields");
            loaded.values.push(RankValues {
                range: fields.range,
                cooldown: ticks(fields.cooldown_ms)?,
                cost: fields.cost,
                cast_time: ticks(fields.cast_time_ms)?,
            });
            loaded.resource_costs.extend(fields.resource_cost);
        }
        Ok(loaded)
    }
}
