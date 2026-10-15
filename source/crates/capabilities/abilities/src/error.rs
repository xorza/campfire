use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

campfire_common::enum_all! {
    /// Why the abilities capability refused a script call: a script gave it a value it does not take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AbilitiesRefusal {
        /// An ability with no charges.
        NoCharges,
        /// A value of an action's start in a call of an action that did not start, such as a weapon's.
        NoStart,
        /// A fraction below 0 or above 1.
        NotAFraction,
        /// `ctx.charge` of an action that does not charge.
        NotCharged,
        /// An ability the unit does not hold.
        NotHeld,
    }
    const ALL;
}

impl CapabilityRefusal for AbilitiesRefusal {
    const OWNER: Capability = Capability::Abilities;
    const CASES: &'static [AbilitiesRefusal] = &AbilitiesRefusal::ALL;

    fn message(self) -> &'static str {
        match self {
            AbilitiesRefusal::NoCharges => "the ability has no charges",
            AbilitiesRefusal::NoStart => "the call's action did not start",
            AbilitiesRefusal::NotAFraction => "fraction is below 0 or above 1",
            AbilitiesRefusal::NotCharged => "the call's action does not charge",
            AbilitiesRefusal::NotHeld => "the unit does not hold the ability",
        }
    }
}
