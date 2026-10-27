use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A button's image on the command panel: the rectangle `rect` of the package's texture
/// `texture`, which the client draws through Bevy UI's image node, so a game's atlas of images
/// converts once (02-engine-core.md, An icon is a rectangle of a texture).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelIcon {
    pub texture: PackagePath,
    pub rect: IconRect,
}

/// A rectangle of a texture in pixels, from its left and top edges to its right and bottom ones,
/// written as `[left, top, right, bottom]`, of at least a pixel each way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "[u16; 4]")]
pub struct IconRect {
    left: u16,
    top: u16,
    right: u16,
    bottom: u16,
}

impl IconRect {
    /// The rectangle from `left` and `top` to `right` and `bottom`; none of no pixel each way.
    pub const fn new(left: u16, top: u16, right: u16, bottom: u16) -> Option<Self> {
        if right > left && bottom > top {
            Some(Self {
                left,
                top,
                right,
                bottom,
            })
        } else {
            None
        }
    }

    /// Its edges, left, top, right and bottom.
    pub const fn edges(self) -> [u16; 4] {
        [self.left, self.top, self.right, self.bottom]
    }
}

impl From<IconRect> for [u16; 4] {
    fn from(rect: IconRect) -> Self {
        rect.edges()
    }
}

/// A package's file is untrusted, so a rectangle of no pixel fails to read.
impl<'de> Deserialize<'de> for IconRect {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [left, top, right, bottom] = <[u16; 4]>::deserialize(deserializer)?;
        Self::new(left, top, right, bottom).ok_or_else(|| {
            D::Error::custom("an icon's rectangle runs right of its left and below its top")
        })
    }
}
