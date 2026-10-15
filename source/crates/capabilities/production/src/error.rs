use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

campfire_common::enum_all! {
    /// Why the production capability refused a script call: a script gave it a value it does not
    /// take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProductionRefusal {
        /// A count of supply in a mode that counts none.
        NoSupply,
    }
    const ALL;
}

impl CapabilityRefusal for ProductionRefusal {
    const OWNER: Capability = Capability::Production;
    const CASES: &'static [ProductionRefusal] = &ProductionRefusal::ALL;

    fn message(self) -> &'static str {
        match self {
            ProductionRefusal::NoSupply => "the mode counts no supply",
        }
    }
}
