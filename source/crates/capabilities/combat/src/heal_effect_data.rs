use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declarations::{BookError, Listed};
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, CallError, CapabilityRefusal, Ctx, Frame, MemberDecl, Unit, View,
};
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use campfire_values::Number;
use serde::Deserialize;

use crate::combat_effect::CombatEffect;
use crate::error::CombatRefusal;

/// `heal = { amount }`, of the life pool, which joins the damage pass.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealEffectData {
    pub amount: Number,
}

/// A listed heal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HealEffectSpec {
    amount: Amount,
}

impl Listed for HealEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "heal";

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(api: &mut ApiBuilder<'_>) {
        let heal = MemberDecl::call(
            "heal",
            &[&["unit", "amount"]],
            "heals `unit`'s life pool, times one plus its `heal_scale` stat",
        );
        api.call(heal, |ctx: &mut Ctx, unit: Unit, amount: Num| {
            if amount < Num::ZERO {
                return Err(CombatRefusal::NegativeHeal.fail().into());
            }
            ctx.queue(CombatEffect::Heal {
                unit: unit.id,
                amount,
            })
        });
    }
}

impl ListedEffect for HealEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Combat)
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
        Ok(Arc::new(HealEffectSpec {
            amount: cx.amount(&self.amount)?,
        }))
    }
}

impl ListedSpec for HealEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, _: &View) -> Result<(), CallError> {
        frame.queue(CombatEffect::Heal {
            unit: at.unit(frame),
            amount: self.amount.number(frame),
        });
        Ok(())
    }
}
