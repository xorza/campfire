use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, RangeTakes, Takes};
use campfire_common::DeclaredName;
use campfire_declare::Listed;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked, Share};
use serde::{Deserialize, Serialize};

use crate::build_start::BuildStart;
use crate::construct_data::ConstructData;
use crate::placement_data::PlacementData;
use crate::requires_data::RequiresData;

/// The section of an action of kind `build`: the unit type of its package it places, what its
/// player must hold for it to start, how its site grows, when it starts, as its builder arrives
/// by default, the share of its life pool's maximum its site starts with, the share of the player
/// resources it paid that a cancel returns, all when absent, and where it may place its box
/// (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildData {
    pub unit_type: DeclaredName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<RequiresData>,
    pub construct: ConstructData,
    #[serde(default)]
    pub start: BuildStart,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_life: Option<Share>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_refund: Option<Share>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementData>,
}

impl KindSection for BuildData {
    fn kind(&self) -> ActionKind {
        ActionKind::Build
    }

    /// Navigation tests a build's box for room and walks its builder.
    fn needs(&self) -> &'static [Capability] {
        &[Capability::Production, Capability::Navigation]
    }

    /// A build aims at a point within its range in meters, and its windup is its site's time.
    fn takes(&self) -> KindTakes {
        KindTakes {
            range: RangeTakes::Meters,
            cooldown: Takes::May,
            cost: Takes::May,
            windup: Takes::Must,
            passive: Takes::May,
            ..KindTakes::NONE
        }
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }
}

impl Listed for BuildData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Build.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
