use std::any::Any;
use std::fmt::Debug;

use campfire_declare::ListedSet;
use campfire_script::DataTable;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};

use crate::action_kind::ActionKind;
use crate::dyn_eq::DynEq;
use crate::kind_takes::KindTakes;

/// What an action's data gives for its kind, beside the pipeline's fields: the type of the module
/// that registers the kind by its name, which reads the fields of the kind's own and builds what
/// the kind runs from them. No type of the core lists a kind's fields
/// (04-capabilities/actions.md, Each kind owns its section).
pub trait KindSection: DynEq + Any + Debug + Send + Sync {
    /// Its kind, whose name its module registers it by.
    fn kind(&self) -> ActionKind;

    /// The capabilities the mode declares for the kind to run.
    fn needs(&self) -> &'static [Capability];

    /// The pipeline's fields its kind takes.
    fn takes(&self) -> KindTakes;

    /// Its numbers at each rank, which count the action's ranks and may read its params.
    fn numbers(&self) -> Vec<&Ranked<Number>>;
}

impl PartialEq for dyn KindSection {
    fn eq(&self, other: &Self) -> bool {
        self.eq_dyn(other)
    }
}

impl Eq for dyn KindSection {}

/// The open set of the kinds' sections, each by its kind's name.
#[derive(Debug)]
pub struct KindSections;

impl ListedSet for KindSections {
    type Value = dyn KindSection;

    const TABLE: DataTable = DataTable::new("An action's `kind`");
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use campfire_declare::{Listed, SectionReaders};
    use campfire_script::ApiBuilder;
    use campfire_sim::Capability;
    use campfire_values::{Number, Ranked};
    use serde::Deserialize;

    use crate::action_kind::ActionKind;
    use crate::kind_section::{KindSection, KindSections};
    use crate::kind_takes::KindTakes;

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
    }

    impl Listed for TestKind {
        type Set = KindSections;

        const KEY: &'static str = ActionKind::Cast.name();

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
}
