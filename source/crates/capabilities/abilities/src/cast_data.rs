use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes};
use campfire_declarations::Listed;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::Deserialize;

/// The section of an action of kind `cast`, the default kind: a cast has no field of its own,
/// and runs every field of the pipeline (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "serde reads a struct of braces from a table, and a unit struct from no value"
)]
pub struct CastData {}

impl KindSection for CastData {
    fn kind(&self) -> ActionKind {
        ActionKind::Cast
    }

    fn needs(&self) -> &'static [Capability] {
        &[Capability::Abilities]
    }

    fn takes(&self) -> KindTakes {
        KindTakes::ANY
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        Vec::new()
    }
}

impl Listed for CastData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Cast.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
