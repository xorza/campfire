use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// What a look's condition tests of a unit, of what every client holds of it, as a units file
/// names each condition (animated-looks.md, A condition is the game's name for a test).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "test", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConditionTest {
    /// The map holds a condition of its name, as a night map holds `night`.
    Map,
    /// The unit holds `tag`.
    Tag { tag: DeclaredName },
    /// The unit walks to a destination, and its place moved within the last `still_ms`
    /// milliseconds, so a walker held up for longer stands.
    Moving { still_ms: u32 },
    /// The unit's life is of a share of its most within its bounds.
    Life(LifeShare),
    /// No unit holds it: a condition of a game that no test of the engine's says, which a state
    /// counts all the same, so it is drawn only where no state of fewer such conditions matches.
    Never,
}

/// A share of a unit's most life: above `above`, and at most `at_most`, from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LifeShare {
    above: Num,
    at_most: Num,
}

impl LifeShare {
    /// The share above `above` and at most `at_most`; none unless `0 ≤ above < at_most ≤ 1`.
    pub fn new(above: Num, at_most: Num) -> Option<Self> {
        (Num::ZERO <= above && above < at_most && at_most <= Num::ONE)
            .then_some(Self { above, at_most })
    }

    pub const fn above(self) -> Num {
        self.above
    }

    pub const fn at_most(self) -> Num {
        self.at_most
    }

    /// Whether a life of `current` of a most of `max` lies within it: `above · max < current ≤
    /// at_most · max`, of the exact products, so no share is rounded; a most of none holds no
    /// share.
    pub fn holds(self, current: Num, max: Num) -> bool {
        let of_max = |share: Num| i128::from(share.to_bits()) * i128::from(max.to_bits());
        let current = i128::from(current.to_bits()) << Num::FRAC_BITS;
        max > Num::ZERO && of_max(self.above) < current && current <= of_max(self.at_most)
    }
}

/// A package's file is untrusted, so bounds out of order or past 0 and 1 fail to read.
impl<'de> Deserialize<'de> for LifeShare {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            above: Num,
            at_most: Num,
        }
        let Fields { above, at_most } = Fields::deserialize(deserializer)?;
        Self::new(above, at_most)
            .ok_or_else(|| D::Error::custom("a life share's bounds do not rise within 0 and 1"))
    }
}
