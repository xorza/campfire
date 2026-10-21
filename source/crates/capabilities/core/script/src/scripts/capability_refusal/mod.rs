use std::fmt;

use campfire_sim::Capability;
use rhai::EvalAltResult;

use crate::scripts::error::{ApiError, Refusal};

/// A capability's own refusals of script calls: a closed set of cases, each with its message,
/// which the script API carries as a `Refusal` of the capability.
pub trait CapabilityRefusal: Copy + PartialEq + fmt::Debug + 'static {
    /// The capability whose calls refuse with it.
    const CAPABILITY: Capability;
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
        ApiError::Refused(Refusal::new(Self::CAPABILITY, code, self.message()))
    }

    /// The error that fails the call with this refusal.
    fn fail(self) -> EvalAltResult {
        self.refused().fail()
    }
}

#[cfg(test)]
mod tests;
