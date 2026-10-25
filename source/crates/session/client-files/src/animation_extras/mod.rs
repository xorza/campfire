use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// What a glTF animation of a package says past glTF's own animation, as the vendor extension
/// `AnimationExtras::EXTENSION` on it: its frames and their rate a second, on which its tracks'
/// keys fall, and the frames each node's own meshes are hidden in, as glTF animates no visibility
/// (02-engine-core.md, A visibility channel hides its pivot's own meshes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnimationExtras {
    pub frames: u32,
    pub rate: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<HiddenFrames>,
}

/// The frames the node `node`'s own meshes are hidden in, the frame `f` drawn from the time
/// `f / rate` to `(f + 1) / rate`: runs, each from its first frame to before its end, in order,
/// none touching the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiddenFrames {
    pub node: u32,
    pub runs: Vec<[u32; 2]>,
}

impl AnimationExtras {
    /// The name of the glTF extension that holds an animation's extras.
    pub const EXTENSION: &str = "CAMPFIRE_animation_extras";
}

/// A package's file is untrusted, so extras of no frame, of a rate of 0, of a node named twice or
/// out of order, or of a run empty, past the last frame, out of order or touching the next fail
/// to read.
impl<'de> Deserialize<'de> for AnimationExtras {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            frames: u32,
            rate: u32,
            #[serde(default)]
            hidden: Vec<HiddenFrames>,
        }
        let Fields {
            frames,
            rate,
            hidden,
        } = Fields::deserialize(deserializer)?;
        if frames == 0 || rate == 0 {
            return Err(D::Error::custom("an animation has no frame or a rate of 0"));
        }
        if !hidden.is_sorted_by(|first, next| first.node < next.node) {
            return Err(D::Error::custom("a node's hidden frames are not in order"));
        }
        for node in &hidden {
            let runs_hold = node
                .runs
                .iter()
                .all(|&[first, end]| first < end && end <= frames)
                && node.runs.is_sorted_by(|[_, end], [next, _]| end < next);
            if !runs_hold {
                return Err(D::Error::custom(
                    "a run of hidden frames is empty, past the last frame or out of order",
                ));
            }
        }
        Ok(Self {
            frames,
            rate,
            hidden,
        })
    }
}

#[cfg(test)]
mod tests;
