use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, Takes};
use campfire_common::DeclaredName;
use campfire_declare::Listed;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::{Deserialize, Serialize};

use crate::requires_data::RequiresData;

/// The section of an action of kind `train`: the unit type of its package it makes, and what its
/// player must hold for it to join a queue (04-capabilities/actions.md, Each kind owns its
/// section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainData {
    pub unit_type: DeclaredName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<RequiresData>,
}

impl KindSection for TrainData {
    fn kind(&self) -> ActionKind {
        ActionKind::Train
    }

    fn needs(&self) -> &'static [Capability] {
        &[Capability::Production]
    }

    /// A train aims at nothing and runs no script: it queues, waits its windup, and spawns.
    fn takes(&self) -> KindTakes {
        KindTakes {
            cooldown: Takes::May,
            cost: Takes::May,
            windup: Takes::May,
            passive: Takes::May,
            ..KindTakes::NONE
        }
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }
}

impl Listed for TrainData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Train.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
