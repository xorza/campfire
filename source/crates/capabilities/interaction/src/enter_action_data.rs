use std::sync::Arc;

use campfire_actions::{
    ActionKind, KindSection, KindSections, KindTakes, ListedSpec, PipelineField, Takes,
};
use campfire_declarations::Listed;
use campfire_script::ApiBuilder;
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::{Deserialize, Serialize};

use crate::entering::EntryResolve;

/// The section of an action of kind `enter`, a container's entry: it has no field of its own. The
/// abilities' casts run it as a use, within a range in meters of the container's body, and its
/// resolve puts its rider in the container (04-capabilities/interaction.md, An entry is an action
/// of the container's), so it takes a windup, and no script, list, cost, channel, cooldown or
/// delivery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "serde reads a struct of braces from a table, and a unit struct from no value"
)]
pub struct EnterActionData {}

impl KindSection for EnterActionData {
    fn kind(&self) -> ActionKind {
        ActionKind::Enter
    }

    fn needs(&self) -> &'static [Capability] {
        &[
            Capability::Interaction,
            Capability::Abilities,
            Capability::Orders,
        ]
    }

    fn takes(&self) -> KindTakes {
        KindTakes::NONE
            .with(Takes::Must, &[PipelineField::Range])
            .with(Takes::May, &[PipelineField::WindupMs])
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }

    fn resolves(&self) -> Option<Arc<dyn ListedSpec>> {
        Some(Arc::new(EntryResolve))
    }
}

impl Listed for EnterActionData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Enter.name();

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
