use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, CallError, CapabilityRefusal, Checked, Ctx, Frame, MemberSpec, Unit, View,
};
use campfire_sim::{Capability, StableId};
use campfire_values::{DamageKind, Number};
use serde::Deserialize;

use crate::combat_effect::CombatEffect;
use crate::error::CombatRefusal;

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

    fn api(api: &mut ApiBuilder<'_>) {
        let damage = MemberSpec::call(
            "damage",
            &[
                &["unit", "kind", "amount"],
                &["unit", "kind", "amount", "source"],
            ],
            "deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `unit`, from \
             `source`, else from the acting unit",
        );
        api.call(
            damage,
            |ctx: &mut Ctx, unit: Unit, kind: DamageKind, amount: Num| {
                deal(ctx, &unit, kind, amount, None)
            },
        )
        .call(
            damage,
            |ctx: &mut Ctx, unit: Unit, kind: DamageKind, amount: Num, source: Unit| {
                deal(ctx, &unit, kind, amount, Some(source.id))
            },
        );
    }
}

/// Queues `amount`, not negative, of `kind` damage to `target`, from `source`, else from the
/// acting unit.
fn deal(
    ctx: &Ctx,
    target: &Unit,
    kind: DamageKind,
    amount: Num,
    source: Option<StableId>,
) -> Checked<()> {
    if amount < Num::ZERO {
        return Err(CombatRefusal::NegativeDamage.fail().into());
    }
    ctx.queue(CombatEffect::Damage {
        unit: target.id,
        amount,
        kind,
        source,
    })
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
            source: None,
        });
        Ok(())
    }
}
