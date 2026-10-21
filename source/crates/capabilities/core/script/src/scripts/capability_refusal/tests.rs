use campfire_common::EnumAll;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
enum Sample {
    First,
    Second,
}

impl CapabilityRefusal for Sample {
    const CAPABILITY: Capability = Capability::Combat;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::First => "the first",
            Self::Second => "the second",
        }
    }
}

/// The first case's code under another capability: the same code and message, another owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Other;

impl CapabilityRefusal for Other {
    const CAPABILITY: Capability = Capability::Stats;
    const CASES: &'static [Self] = &[Self];

    fn message(self) -> &'static str {
        "the first"
    }
}

#[test]
fn a_refusal_is_its_capability_and_case_and_says_its_message() {
    // A case is equal to itself alone: `First` is code 0 of combat, `Second` code 1, and
    // stats' code 0 is another refusal though its message is combat's code 0's.
    assert_eq!(Sample::First.refused(), Sample::First.refused());
    assert_ne!(Sample::First.refused(), Sample::Second.refused());
    assert_ne!(Sample::First.refused(), Other.refused());
    assert_eq!(
        Sample::First.refused(),
        ApiError::Refused(Refusal::new(Capability::Combat, 0, "the first"))
    );
    assert_eq!(
        Sample::Second.refused(),
        ApiError::Refused(Refusal::new(Capability::Combat, 1, "the second"))
    );
    assert_eq!(Sample::Second.refused().to_string(), "the second");
}
