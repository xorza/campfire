use campfire_math::Num;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

/// The wrecks a death's phase throws, of one of the game's creation lists, of which a death
/// throws one list drawn at random (wrecks.md, A death names its wrecks by phase).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WreckList(pub Vec<Wreck>);

/// Wrecks of one kind a phase throws: `count` pieces, each of one of its `models` drawn at
/// random, from `offset` of its unit's drawing, in metres along its facing, across it and up,
/// launched by `launch`, spun at most `spin` radians a second about each axis, and sunk as `sink`
/// says (wrecks.md, A wreck is the client's; wrecks.md, Wrecks move by the client's step).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wreck {
    pub models: Vec<PackagePath>,
    pub count: u8,
    pub offset: [Num; 3],
    pub launch: WreckLaunch,
    pub spin: Num,
    pub sink: WreckSink,
}

/// How a wreck leaves its unit, each velocity in metres a second, as one step's change of
/// velocity the game's force over its mass makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WreckLaunch {
    /// Where it was made, still.
    Still,
    /// On the ground beneath, turned at random, still.
    Grounded,
    /// On the ground, slid across it, each of the two ground axes within ±`across`.
    Out { across: Num },
    /// Thrown: each ground axis within ±`across`, and up between `up[0]` and `up[1]`.
    Fling { across: Num, up: [Num; 2] },
    /// Thrown at a speed between `speed[0]` and `speed[1]`, at a pitch above the ground between
    /// `pitch[0]` and `pitch[1]` radians, in a direction drawn at random.
    Force { speed: [Num; 2], pitch: [Num; 2] },
}

/// How a wreck ends: a hulk after its `life` in seconds, a piece of debris as it rests; then it
/// sinks at `rate` metres a second after `delay` seconds, and goes `gone` seconds after its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WreckSink {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub life: Option<Num>,
    pub delay: Num,
    pub rate: Num,
    pub gone: Num,
}

impl WreckList {
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
