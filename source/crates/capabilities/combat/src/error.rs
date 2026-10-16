use campfire_derive::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

/// Why the combat capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum CombatRefusal {
    NegativeDamage,
    /// A heal or restore of a negative amount.
    NegativeHeal,
}

impl CapabilityRefusal for CombatRefusal {
    const OWNER: Capability = Capability::Combat;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeDamage => "damage is negative",
            Self::NegativeHeal => "amount to heal or restore is negative",
        }
    }
}
