use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_script::{CallError, Frame, View};
use campfire_sim::Capability;
use campfire_values::Number;
use serde::Deserialize;

use crate::combat_effect::CombatEffect;

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

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
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
