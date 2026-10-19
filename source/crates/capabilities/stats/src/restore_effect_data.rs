use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, CallError, CapabilityRefusal, Ctx, Frame, MemberSpec, Unit, View,
};
use campfire_sim::Capability;
use campfire_units::PoolId;
use campfire_values::Number;
use serde::Deserialize;

use crate::error::StatsRefusal;
use crate::stats_effect::StatsEffect;

/// `restore = { pool, amount }`: `amount` of the unit's `pool`, unscaled.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreEffectData {
    pub pool: DeclaredName,
    pub amount: Number,
}

/// A listed restore, its names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RestoreEffectSpec {
    pool: PoolId,
    amount: Amount,
}

impl Listed for RestoreEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "restore";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(api: &mut ApiBuilder<'_>) {
        let restore = MemberSpec::call(
            "restore",
            &[&["unit", "pool", "amount"]],
            "gives `unit` back `amount` of its `pool`, unscaled, when it lives",
        );
        api.call(
            restore,
            |ctx: &mut Ctx, unit: Unit, pool: PoolId, amount: Num| {
                if amount < Num::ZERO {
                    return Err(StatsRefusal::NegativeRestore.fail().into());
                }
                ctx.queue(StatsEffect::Restore {
                    unit: unit.id,
                    pool,
                    amount,
                })
            },
        );
    }
}

impl ListedEffect for RestoreEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Stats)
    }

    fn numbers(&self) -> Vec<&Number> {
        vec![&self.amount]
    }

    fn duration(&self) -> Option<&Number> {
        None
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        None
    }

    fn lists(&self) -> Option<&LaunchLists> {
        None
    }

    fn takes_point(&self) -> bool {
        false
    }

    fn rule(&self, _: &dyn EffectRuleCx) -> Result<(), BookError> {
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        Ok(Arc::new(RestoreEffectSpec {
            pool: cx.names().pool(&self.pool)?,
            amount: cx.amount(&self.amount)?,
        }))
    }
}

impl ListedSpec for RestoreEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, _: &View) -> Result<(), CallError> {
        frame.queue(StatsEffect::Restore {
            unit: at.unit(frame),
            pool: self.pool,
            amount: self.amount.number(frame),
        });
        Ok(())
    }
}
