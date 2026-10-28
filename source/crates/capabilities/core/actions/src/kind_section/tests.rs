use std::sync::Arc;

use campfire_declarations::{Listed, SectionReaders};
use campfire_script::ApiBuilder;
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};
use serde::Deserialize;

use crate::action_kind::ActionKind;
use crate::kind_section::{KindSection, KindSections};
use crate::kind_takes::KindTakes;
use crate::listed_effect::listed_spec::ListedSpec;

/// The kind of the crate's tests, registered as `cast`: it may take every field of the
/// pipeline, and has one number of its own, `most`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestKind {
    pub(crate) most: Option<Ranked<Number>>,
}

impl KindSection for TestKind {
    fn kind(&self) -> ActionKind {
        ActionKind::Cast
    }

    fn needs(&self) -> &'static [Capability] {
        &[]
    }

    fn takes(&self) -> KindTakes {
        KindTakes::ANY
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        self.most.iter().collect()
    }

    fn resolves(&self) -> Option<Arc<dyn ListedSpec>> {
        None
    }
}

impl Listed for TestKind {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Cast.name();

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

impl TestKind {
    /// The readers of the crate's tests: its kind alone.
    pub(crate) fn readers() -> SectionReaders {
        let mut readers = SectionReaders::default();
        readers.add_listed::<Self>();
        readers
    }
}
