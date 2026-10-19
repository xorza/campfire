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
use campfire_values::{Number, TrackId};
use serde::Deserialize;

use crate::progression_effect::ProgressionEffect;

/// `xp = { track, amount }`: experience on a track the unit has.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpEffectData {
    pub track: DeclaredName,
    pub amount: Number,
}

/// A listed experience, its track resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct XpEffectSpec {
    track: TrackId,
    amount: Amount,
}

impl Listed for XpEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "xp";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(api: &mut ApiBuilder<'_>) {
        let add_xp = MemberSpec::call(
            "add_xp",
            &[&["unit", "track", "amount"]],
            "gives `unit` `amount` of experience on `track`, one of its unit type's",
        );
        api.call(
            add_xp,
            |ctx: &mut Ctx, unit: Unit, track: TrackId, amount: Num| {
                let effect = ProgressionEffect::add_xp(ctx.view(), unit.id, track, amount);
                ctx.queue(effect.map_err(CapabilityRefusal::fail)?)
            },
        );
    }
}

impl ListedEffect for XpEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Progression)
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
        Ok(Arc::new(XpEffectSpec {
            track: cx.names().track(&self.track)?,
            amount: cx.amount(&self.amount)?,
        }))
    }
}

impl ListedSpec for XpEffectSpec {
    /// A unit `view` does not hold, or that does not have the track, fails the call, as
    /// `ctx.add_xp` does.
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let amount = self.amount.number(frame);
        let effect = ProgressionEffect::add_xp(view, at.unit(frame), self.track, amount);
        frame.queue(effect.map_err(|refusal| CallError::Api(refusal.refused()))?);
        Ok(())
    }
}
