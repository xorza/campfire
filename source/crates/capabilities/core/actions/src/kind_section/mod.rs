use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

use campfire_declarations::ListedSet;
use campfire_script::DataTable;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked};

use crate::action_kind::ActionKind;
use crate::dyn_eq::DynEq;
use crate::kind_takes::KindTakes;
use crate::listed_effect::listed_spec::ListedSpec;

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

    /// The effect its kind adds to the end of its action's `on_resolve`, to the unit the action
    /// reached, which no data writes; none for a kind whose resolve is its script's and its
    /// lists' alone.
    fn resolves(&self) -> Option<Arc<dyn ListedSpec>>;
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
pub(crate) mod tests;
