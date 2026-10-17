use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed};
use campfire_script::{CallError, Frame, View};
use campfire_sim::Capability;
use campfire_stats::StatsEffect;
use campfire_values::{Number, Tag};
use serde::Deserialize;

use crate::listed_effect::EffectResolveCx;
use crate::listed_effect::EffectRuleCx;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::{EffectError, EffectRule};
use crate::listed_effect::listed_spec::{ListedAt, ListedSpec};
use crate::listed_effect::{ListedEffect, ListedEffects};

/// `purge = { tag }`: the modifiers that grant the tag, of no engine's tag.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeEffectData {
    pub tag: DeclaredName,
}

/// A listed purge, its tag resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PurgeEffectSpec {
    tag: Tag,
}

impl Listed for PurgeEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "purge";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }
}

impl ListedEffect for PurgeEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Stats)
    }

    fn numbers(&self) -> Vec<&Number> {
        Vec::new()
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

    fn rule(&self, cx: &dyn EffectRuleCx) -> Result<(), BookError> {
        if cx.engine_tag(&self.tag) {
            return Err(cx.fail(EffectRule::EngineTag(self.tag.clone())));
        }
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        Ok(Arc::new(PurgeEffectSpec {
            tag: cx.names().tag(&self.tag)?,
        }))
    }
}

impl ListedSpec for PurgeEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, _: &View) -> Result<(), CallError> {
        frame.queue(StatsEffect::Purge {
            carrier: at.unit(frame),
            tag: self.tag,
        });
        Ok(())
    }
}
