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

/// A helicopter's spiral down: it turns about its path at `turn` radians a second at `speed`
/// metres a second, and spins at `spin` radians a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spiral {
    pub turn: Num,
    pub speed: Num,
    pub spin: Num,
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
