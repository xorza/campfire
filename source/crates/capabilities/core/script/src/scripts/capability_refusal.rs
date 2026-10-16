use std::fmt;

use campfire_sim::Capability;
use rhai::EvalAltResult;

use crate::scripts::error::{ApiError, Refusal};

/// A capability's own refusals of script calls: a closed set of cases, each with its message,
/// which the script API carries as a `Refusal` of the capability.
pub trait CapabilityRefusal: Copy + PartialEq + fmt::Debug + 'static {
    /// The capability whose calls refuse with it.
    const OWNER: Capability;
    /// Every case, each at its code.
    const CASES: &'static [Self];

    /// Why the call was refused.
    fn message(self) -> &'static str;

    /// The script API's error of this refusal: its code is its place among the cases.
    fn refused(self) -> ApiError {
        let at = Self::CASES
            .iter()
            .position(|listed| *listed == self)
            .expect("a refusal's cases list each of its cases");
        let code = u8::try_from(at).expect("a capability refuses in at most 256 ways");
        ApiError::Refused(Refusal::new(Self::OWNER, code, self.message()))
    }

    /// The error that fails the call with this refusal.
    fn fail(self) -> EvalAltResult {
        self.refused().fail()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    campfire_common::enum_all! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Sample {
            First,
            Second,
        }
        const ALL;
    }

    impl CapabilityRefusal for Sample {
        const OWNER: Capability = Capability::Combat;
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
        const OWNER: Capability = Capability::Stats;
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
}
