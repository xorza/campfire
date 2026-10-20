use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, RangeTakes, Takes};
use campfire_common::DeclaredName;
use campfire_declarations::Listed;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Distance, Number, Ranked};
use serde::{Deserialize, Serialize};

/// The section of an action of kind `gather`: the mode's player resource it gathers, the most a
/// trip carries, what each windup takes of it, the whole trip's when absent, how far from its
/// node it looks for another, none when absent, and how long its worker stands at a drop-off
/// before its load joins its player's, no time when absent (04-capabilities/actions.md, Each kind
/// owns its section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatherData {
    pub resource: DeclaredName,
    pub take: NonZeroU32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<NonZeroU32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounce: Option<Distance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliver_ms: Option<u64>,
}

impl KindSection for GatherData {
    fn kind(&self) -> ActionKind {
        ActionKind::Gather
    }

    /// Navigation walks a gatherer between its node and its drop-off.
    fn needs(&self) -> &'static [Capability] {
        &[Capability::Production, Capability::Navigation]
    }

    /// A gather aims at a node within its range in meters, a trip takes its windup, and it pays
    /// nothing.
    fn takes(&self) -> KindTakes {
        KindTakes {
            range: RangeTakes::Meters,
            windup: Takes::Must,
            passive: Takes::May,
            ..KindTakes::NONE
        }
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }
}

impl Listed for GatherData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Gather.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
