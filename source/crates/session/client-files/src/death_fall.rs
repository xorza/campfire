use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::{Deserialize, Serialize};

use crate::wreck::WreckList;

/// An aircraft's death, which falls as a wreck: its drawing leaves its unit, plays `start` and
/// falls at `gravity` times the effects' gravity, rolling at `roll` radians a second, or
/// spiralling as `spiral` says; it plays `hit` as it meets the ground and `blow_up`
/// `blow_up_after` seconds after; a jet that dies on the ground falls not and plays its `ground`
/// death alone (02-engine-core.md, An aircraft's death falls as a wreck).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathFall {
    pub gravity: Num,
    pub roll: Num,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spiral: Option<Spiral>,
    #[serde(default, skip_serializing_if = "FallStage::is_empty")]
    pub start: FallStage,
    #[serde(default, skip_serializing_if = "FallStage::is_empty")]
    pub hit: FallStage,
    pub blow_up_after: Num,
    #[serde(default, skip_serializing_if = "FallStage::is_empty")]
    pub blow_up: FallStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<GroundDeath>,
}

/// A jet's death on the ground: it dies there when it stands at most `height` metres above the
/// ground, and plays `stage` in place of its fall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundDeath {
    pub height: Num,
    #[serde(default, skip_serializing_if = "FallStage::is_empty")]
    pub stage: FallStage,
}

/// A helicopter's spiral down, as Zero Hour's `HelicopterSlowDeathBehavior` pushes it: from the
/// velocity it died with, each step a push of `push` metres a second squared along a way that
/// starts at its facing and turns left `turn` radians a second, the push kept `fade` of itself
/// each step; its hover's brake along its facing, by up to `braking` metres a second squared
/// forward and `acceleration` backward, as a hover with no goal holds its place; and its own spin,
/// `spin` (02-engine-core.md, An aircraft's death falls as a wreck).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spiral {
    pub push: Num,
    pub turn: Num,
    pub fade: Num,
    pub braking: Num,
    pub acceleration: Num,
    pub spin: SelfSpin,
}

/// A spiral's own spin, in radians a second: it starts at `least` and changes by `change` each
/// time more than `every` seconds passed since its last change, toward `most`, then back toward
/// `least` once it reaches it, and on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfSpin {
    pub least: Num,
    pub most: Num,
    pub change: Num,
    pub every: Num,
}

/// What a moment of a fall plays: its effects, of which it plays one drawn at random, each by its
/// name in the effects file, and its wrecks, of which it throws one list drawn at random.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallStage {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrecks: Vec<WreckList>,
}

impl FallStage {
    pub const fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.wrecks.is_empty()
    }
}
