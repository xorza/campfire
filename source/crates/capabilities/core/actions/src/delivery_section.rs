use std::any::Any;
use std::fmt::Debug;

use campfire_common::DeclaredName;
use campfire_declarations::ListedSet;
use campfire_script::DataTable;
use campfire_sim::Capability;

use crate::dyn_eq::DynEq;
use crate::fan::Fan;

/// What an action's `delivery` table gives: the type of the module that delivers it, which
/// registers its form by the key that names the unit type it launches, reads the rest of the
/// table, and says what the delivery takes. No type of the core lists a form or its fields
/// (04-capabilities/actions.md, A delivery's module owns its form).
pub trait DeliverySection: DynEq + Any + Debug + Send + Sync {
    /// The capability that delivers it, which the mode declares.
    fn capability(&self) -> Capability;

    /// The name of the unit type it launches, of its package.
    fn unit_type(&self) -> &DeclaredName;

    /// How many it launches at once, and their spread.
    fn fan(&self) -> Fan;
}

impl PartialEq for dyn DeliverySection {
    fn eq(&self, other: &Self) -> bool {
        self.eq_dyn(other)
    }
}

impl Eq for dyn DeliverySection {}

/// The open set of the delivery forms, each by the key that names its unit type.
#[derive(Debug)]
pub struct DeliverySections;

impl ListedSet for DeliverySections {
    type Value = dyn DeliverySection;

    const TABLE: DataTable = DataTable::new("An action's `delivery`");
}
