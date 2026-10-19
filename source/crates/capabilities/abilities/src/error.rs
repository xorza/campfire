use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

/// Why the abilities capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum AbilitiesRefusal {
    /// An ability with no charges.
    NoCharges,
    /// A value of an action's start in a call of an action that did not start, such as a
    /// weapon's.
    NoStart,
    /// A fraction below 0 or above 1.
    NotAFraction,
    /// `ctx.charge` of an action that does not charge.
    NotCharged,
    /// An ability the unit does not hold.
    NotHeld,
}

impl CapabilityRefusal for AbilitiesRefusal {
    const CAPABILITY: Capability = Capability::Abilities;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NoCharges => "the ability has no charges",
            Self::NoStart => "the call's action did not start",
            Self::NotAFraction => "fraction is below 0 or above 1",
            Self::NotCharged => "the call's action does not charge",
            Self::NotHeld => "the unit does not hold the ability",
        }
    }
}
