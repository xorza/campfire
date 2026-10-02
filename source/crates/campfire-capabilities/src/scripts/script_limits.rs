use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// How many operations scripts may run: in one call, and in each pool's calls of one tick
/// together. Each player, AI and the mode draw from pools of their own, so no call can spend
/// what another's needs, and the sum of the pools is the most one tick's scripts run. The mode's
/// manifest sets them, and each pool holds a whole call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    pub per_call: u64,
    /// For the calls each player causes: their casts and mode inputs. Each player slot has a
    /// pool of this size.
    pub player: u64,
    /// For AI `on_think` calls.
    pub think: u64,
    /// For the mode's own calls: the match start and timers.
    pub mode: u64,
}

impl ScriptLimits {
    /// The chain depth at which a hook fails instead of running: no designed chain of combat
    /// events is that deep, and the damage pass must end within its tick.
    pub(crate) const CHAIN_DEPTH: u8 = 16;

    /// Whether a call may run at least one operation, and each pool holds a whole call.
    const fn holds_calls(self) -> bool {
        self.per_call >= 1
            && self.player >= self.per_call
            && self.think >= self.per_call
            && self.mode >= self.per_call
    }
}

/// A manifest's limits, refused unless each pool holds a whole call.
impl<'de> Deserialize<'de> for ScriptLimits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ScriptLimits, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            per_call: u64,
            player: u64,
            think: u64,
            mode: u64,
        }
        let Fields {
            per_call,
            player,
            think,
            mode,
        } = Fields::deserialize(deserializer)?;
        let limits = ScriptLimits {
            per_call,
            player,
            think,
            mode,
        };
        if !limits.holds_calls() {
            return Err(D::Error::custom(
                "a script pool holds less than a whole call",
            ));
        }
        Ok(limits)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::scripts::script_limits::ScriptLimits;

    impl ScriptLimits {
        /// Limits no test's script comes near, but one that spins without end.
        pub(crate) const ROOMY: ScriptLimits = ScriptLimits {
            per_call: 10_000,
            player: 100_000,
            think: 100_000,
            mode: 100_000,
        };
    }
}
