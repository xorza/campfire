use bevy::animation::graph::{AnimationGraph, AnimationNodeIndex};
use bevy::asset::{Asset, AssetId, AssetPath, AssetServer, Assets, Handle};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::platform::collections::HashMap;
use bevy::reflect::TypePath;

/// The clip of each state's animation the client played, by the path of its file, each loaded
/// once and kept for the match: its graph of it alone, which a model's player plays, its node in
/// it, and its timing. A clip loads as a unit first plays it.
#[derive(Resource, Debug, Default)]
pub(crate) struct LookClips {
    clips: HashMap<AssetPath<'static>, LookClip>,
}

/// A clip as a model's player plays it: its graph, of it alone, its node in it, and its timing.
#[derive(Debug, Clone)]
pub(crate) struct LookClip {
    pub(crate) graph: Handle<AnimationGraph>,
    pub(crate) node: AnimationNodeIndex,
    pub(crate) timing: Handle<ClipTiming>,
}

/// What an animation file says of its clip past its tracks, in its extension
/// `AnimationExtras::EXTENSION`: its frames, their rate a second, and the frames each pivot's own
/// meshes are hidden in, by the pivot's name (02-engine-core.md, A visibility channel hides its
/// pivot's own meshes).
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipTiming {
    pub(crate) frames: u32,
    pub(crate) rate: u32,
    pub(crate) hidden: Vec<HiddenPivot>,
}

/// The frames a pivot's own meshes are hidden in: runs, each from its first frame to before its
/// end, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HiddenPivot {
    pub(crate) pivot: String,
    pub(crate) runs: Vec<[u32; 2]>,
}

/// On a dressed model: the clip whose hidden pivots it holds, by its timing, none before its
/// first; those pivots; and the mesh nodes its clip hides now.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClipHidden {
    pub(crate) timing: Option<AssetId<ClipTiming>>,
    pub(crate) pivots: Vec<ClipPivot>,
    pub(crate) meshes: Vec<Entity>,
}

/// A pivot a clip hides that a model holds: its place among the clip's hidden pivots, and among
/// the model's pivots, of its name, resolved once as the model first plays the clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipPivot {
    pub(crate) hidden: usize,
    pub(crate) pivot: usize,
}

impl LookClips {
    /// The clip of the animation file at `path`, which loads the first time it is asked for, its
    /// graph added to `graphs`.
    pub(crate) fn clip(
        &mut self,
        path: &AssetPath<'static>,
        assets: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> LookClip {
        if let Some(clip) = self.clips.get(path) {
            return clip.clone();
        }
        let animation = assets.load(path.clone().with_label("Animation0"));
        let (graph, node) = AnimationGraph::from_clip(animation);
        let clip = LookClip {
            graph: graphs.add(graph),
            node,
            timing: assets.load(path.clone().with_label("Animation0/timing")),
        };
        self.clips.insert(path.clone(), clip.clone());
        clip
    }
}

impl ClipTiming {
    /// The factor of its rate that plays it once in the time its unit walks `stride` meters at
    /// `speed` meters a second, as `W3DModelDraw::setCurAnimDurationInMsec` sets it: its frames'
    /// time over that time.
    pub(crate) fn walked(&self, stride: f32, speed: f32) -> f32 {
        self.frames as f32 / self.rate as f32 / (stride / speed)
    }
}

impl HiddenPivot {
    /// Whether its pivot's own meshes are hidden at `frame`.
    pub(crate) fn hides(&self, frame: u32) -> bool {
        self.runs
            .iter()
            .any(|&[first, end]| first <= frame && frame < end)
    }
}
