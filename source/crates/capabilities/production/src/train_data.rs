use std::sync::Arc;

use campfire_actions::{
    ActionKind, KindSection, KindSections, KindTakes, ListedSpec, PipelineField, Takes,
};
use campfire_common::DeclaredName;
use campfire_declarations::Listed;
use campfire_script::ApiBuilder;
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::{Deserialize, Serialize};

use crate::requires_data::RequiresData;

/// The section of an action of kind `train`: the unit type of its package it makes, what its
/// player must hold for it to join a queue (04-capabilities/actions.md, Each kind owns its
/// section), and whether it makes its unit at a free place of its producer, which it holds while it
/// lives, where its producer names places (04-capabilities/production.md, A jet holds a place).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainData {
    pub unit_type: DeclaredName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<RequiresData>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_place: bool,
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
        KindTakes::NONE.with(
            Takes::May,
            &[
                PipelineField::CooldownMs,
                PipelineField::Cost,
                PipelineField::WindupMs,
                PipelineField::PassiveModifier,
            ],
        )
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }

    fn resolves(&self) -> Option<Arc<dyn ListedSpec>> {
        None
    }
}

impl Listed for TrainData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Train.name();

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
