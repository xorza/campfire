use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, PipelineField, Takes};
use campfire_declarations::Listed;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::Deserialize;

/// The section of an action of kind `use`, a use of an object: it has no field of its own. The
/// abilities' casts run it, within a range in meters, so it takes a cast's script, cost, windup,
/// channel and hold, and no cooldown, toggle, charge, passive or delivery
/// (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "serde reads a struct of braces from a table, and a unit struct from no value"
)]
pub struct UseActionData {}

impl KindSection for UseActionData {
    fn kind(&self) -> ActionKind {
        ActionKind::Use
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
            .with(
                Takes::May,
                &[
                    PipelineField::Script,
                    PipelineField::Cost,
                    PipelineField::WindupMs,
                    PipelineField::Channel,
                    PipelineField::Hold,
                    PipelineField::Params,
                    PipelineField::OnResolve,
                ],
            )
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }
}

impl Listed for UseActionData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Use.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_use_needs_the_orders_that_fill_its_slot_and_the_abilities_that_run_it() {
        assert_eq!(
            UseActionData {}.needs(),
            [
                Capability::Interaction,
                Capability::Abilities,
                Capability::Orders
            ]
        );
    }
}
