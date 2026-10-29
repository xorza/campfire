use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A model, a `.glb` of the package, the scale it is drawn at, the names of its nodes its
/// default look hides, as the glTF names them, the top of what it draws, the nodes each
/// condition of an upgrade shows and hides over those, in order, the nodes that show the rounds of
/// a weapon slot's clip, and the node of its state's first model it is drawn at, none for one
/// drawn at its unit, as a portable structure is drawn at its container's bone
/// (riders-and-upgrades.md, A model may be drawn at another's bone).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientModel {
    pub model: PackagePath,
    #[serde(default, skip_serializing_if = "ModelScale::is_one")]
    pub scale: ModelScale,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
    /// None for a model that draws nothing, or whose file does not say
    /// (02-engine-core.md, The import measures a look's height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<ModelTop>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upgrades: Vec<NodeUpgrade>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<ClipRounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bone: Option<String>,
}

/// The nodes of a model that show the rounds of the clip of the weapon slot `slot`, by the slot's
/// place among the look's weapons: each round's, in the order its clip fires them, over those its
/// state and its upgrades hide. The client hides one round more for each shot of the clip, and
/// every round while the clip reloads, as Zero Hour hides a launcher's missiles
/// (02-engine-core.md, A model shows its clip's rounds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipRounds {
    pub slot: u8,
    pub rounds: Vec<Vec<String>>,
}

/// The nodes of a model a condition shows and hides while its unit holds it, over those its
/// state hides, as Zero Hour's `SubObjectsUpgrade` shows and hides its object's sub-objects in
/// every state (09-zero-hour.md, An upgrade's look is a tag).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeUpgrade {
    pub condition: DeclaredName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub show: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
}

/// The height in meters of the top of what a model draws over its origin, at its scale, with
/// its hidden nodes hidden: finite, and below the origin for a model drawn under it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelTop(f32);

impl ModelTop {
    /// `None` unless `height` is finite.
    pub const fn new(height: f32) -> Option<Self> {
        if height.is_finite() {
            Some(Self(height))
        } else {
            None
        }
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A top is never NaN.
impl Eq for ModelTop {}

/// A package's file is untrusted, so a top that is not finite fails to read.
impl<'de> Deserialize<'de> for ModelTop {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let height = f32::deserialize(deserializer)?;
        Self::new(height).ok_or_else(|| D::Error::custom("a model's top is finite"))
    }
}

/// The factor a model is drawn at, positive and finite, 1 when a file gives none.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ModelScale(f32);

impl ModelScale {
    pub const ONE: Self = Self(1.0);

    /// `None` unless `factor` is positive and finite.
    pub const fn new(factor: f32) -> Option<Self> {
        if factor.is_finite() && factor > 0.0 {
            Some(Self(factor))
        } else {
            None
        }
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde passes a field by reference"
    )]
    const fn is_one(&self) -> bool {
        self.0 == 1.0
    }
}

/// A scale is never NaN.
impl Eq for ModelScale {}

impl Default for ModelScale {
    fn default() -> Self {
        Self::ONE
    }
}

/// A package's file is untrusted, so a scale that is not positive and finite fails to read.
impl<'de> Deserialize<'de> for ModelScale {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let factor = f32::deserialize(deserializer)?;
        Self::new(factor).ok_or_else(|| D::Error::custom("a model's scale is positive and finite"))
    }
}
