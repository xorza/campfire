use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_script::{CallError, Frame, View};
use campfire_sim::Capability;
use campfire_stats::StatsEffect;
use campfire_values::{ModifierId, Number};
use serde::Deserialize;

use crate::amount::Amount;
use crate::listed_effect::EffectResolveCx;
use crate::listed_effect::EffectRuleCx;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::EffectError;
use crate::listed_effect::listed_spec::{ListedAt, ListedSpec};
use crate::listed_effect::{ListedEffect, ListedEffects};

/// `modifier = { id, duration_ms }`: a modifier of the action's package, from the acting unit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierEffectData {
    pub id: DeclaredName,
    pub duration_ms: Option<Number>,
}

/// A listed modifier, its names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModifierEffectSpec {
    id: ModifierId,
    duration_ms: Option<Amount>,
}

impl Listed for ModifierEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "modifier";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }
}

impl ListedEffect for ModifierEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Stats)
    }

    fn numbers(&self) -> Vec<&Number> {
        self.duration_ms.iter().collect()
    }

    fn duration(&self) -> Option<&Number> {
        self.duration_ms.as_ref()
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        Some(&self.id)
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
        Ok(Arc::new(ModifierEffectSpec {
            id: cx.names().modifier(&self.id)?,
            duration_ms: cx.amount_if(self.duration_ms.as_ref())?,
        }))
    }
}

impl ListedSpec for ModifierEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let duration = self.duration_ms.map(|ms| ms.duration(frame, view.rate()));
        frame.queue(StatsEffect::Add {
            target: at.unit(frame),
            id: self.id,
            duration,
        });
        Ok(())
    }
}
