use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_script::{CallError, Frame, View};
use campfire_sim::Capability;
use campfire_values::{DamageKind, Number};
use serde::Deserialize;

use crate::combat_effect::CombatEffect;

/// `damage = { amount, kind }`: `amount` of `kind` damage, which joins the damage pass.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageEffectData {
    pub amount: Number,
    pub kind: DeclaredName,
}

/// A listed damage, its kind resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DamageEffectSpec {
    amount: Amount,
    kind: DamageKind,
}

impl Listed for DamageEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "damage";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }
}

impl ListedEffect for DamageEffectData {
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
        Ok(Arc::new(DamageEffectSpec {
            amount: cx.amount(&self.amount)?,
            kind: cx.names().damage_kind(&self.kind)?,
        }))
    }
}

impl ListedSpec for DamageEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, _: &View) -> Result<(), CallError> {
        frame.queue(CombatEffect::Damage {
            unit: at.unit(frame),
            amount: self.amount.number(frame),
            kind: self.kind,
        });
        Ok(())
    }
}
