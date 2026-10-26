use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// What a look's condition tests of a unit, of what every client holds of it, as a units file
/// names each condition (09-zero-hour.md, A condition is the game's name for a test).
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
    /// The unit is a site at `stage` of its build.
    Site { stage: SiteStage },
    /// The unit attacks, and a weapon of its look's weapon group at `group` is at `phase`, or at
    /// any phase for none, as Zero Hour's `USING_WEAPON_A` holds beside each.
    Attack {
        group: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<AttackPhase>,
    },
    /// The unit has an attack target, as it approaches, pursues and attacks it, as Zero Hour's
    /// `ATTACKING` holds from its attack state's start to its end (`AIAttackState::onEnter`,
    /// `onExit`).
    Attacking,
    /// The unit is dead, as a type that keeps its dead holds it for its time (04-capabilities/combat.md,
    /// A death keeps its unit for its animation).
    Dead,
}

/// A phase of an attack's weapon, as Zero Hour's `Object::updateDrawableWeaponState` names each:
/// its pre-attack, before its weapon's shot; the tick of its shot; the wait for its next shot; and
/// the reload of its clip, which its last shot emptied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackPhase {
    PreAttack,
    Firing,
    Between,
    Reloading,
}

/// A stage of a site's build: before any builder built it, as Zero Hour's `AWAITING_CONSTRUCTION`,
/// or after, as its `PARTIALLY_CONSTRUCTED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteStage {
    Waiting,
    Started,
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
