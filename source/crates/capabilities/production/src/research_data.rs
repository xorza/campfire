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

/// The section of an action of kind `research`: the modifier of its package it gives, who holds it,
/// and what its player must hold for it to join a queue (04-capabilities/production.md, Research is
/// an action kind of production).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchData {
    pub modifier: DeclaredName,
    #[serde(default)]
    pub holder: ResearchHolder,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<RequiresData>,
}

/// Who a research gives its modifier: its producer's player, which holds it for every unit it
/// selects, or its producer alone, as Zero Hour's player and object upgrades.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchHolder {
    #[default]
    Player,
    Producer,
}

impl KindSection for ResearchData {
    fn kind(&self) -> ActionKind {
        ActionKind::Research
    }

    fn needs(&self) -> &'static [Capability] {
        &[Capability::Production, Capability::Stats]
    }

    /// A research aims at nothing and runs no script: it queues, waits its windup, and gives.
    fn takes(&self) -> KindTakes {
        KindTakes::NONE.with(
            Takes::May,
            &[
                PipelineField::CooldownMs,
                PipelineField::Cost,
                PipelineField::WindupMs,
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

impl Listed for ResearchData {
    type Set = KindSections;
    const KEY: &'static str = ActionKind::Research.name();
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
