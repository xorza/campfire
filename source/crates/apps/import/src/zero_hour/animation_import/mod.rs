use std::collections::{BTreeMap, BTreeSet};

use bevy_math::Vec3;
use campfire_client_files::{AnimationExtras, HiddenFrames};
use campfire_codec::Json;
use gltf_json::animation::{Channel, Interpolation, Property, Sampler, Target};
use gltf_json::extensions::animation::Animation as AnimationExtensions;
use gltf_json::validation::Checked;
use gltf_json::{Accessor, Animation, Extras, Index, Node};

use crate::gltf::glb_builder::GlbBuilder;
use crate::pose::RigidPose;
use crate::quaternion::Quaternion;
use crate::zero_hour::animation_drop_cause::AnimationDropCause;
use crate::zero_hour::axes;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::w3d::hierarchy::{Hierarchy, Pivot};
use crate::zero_hour::format::w3d::w3d_animation::{PivotChannels, W3dAnimation};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::pivot_nodes::PivotNodes;

/// What an animation's name gives: its glTF and its poses at its ends, or why the import drops
/// it.
#[derive(Debug)]
pub(crate) enum ConvertedAnimation {
    Glb {
        glb: Box<GlbBuilder>,
        ends: AnimationEnds,
    },
    Dropped(AnimationDropCause),
}

/// The pose of each pivot of an animation's hierarchy in its parent's frame, at the animation's
/// first frame and at its last, as `Anim_Update` places it, which a model's bones at rest read
/// (`ModelConditionInfo::validateCachedBones`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnimationEnds {
    pub(crate) first: Vec<RigidPose>,
    pub(crate) last: Vec<RigidPose>,
}

/// An animation's glTF as it is built: the animation, its key times by the frames they hold, each
/// written once, its samplers and its channels.
#[derive(Debug)]
struct AnimationBuilder<'a> {
    animation: &'a W3dAnimation,
    glb: GlbBuilder,
    times: BTreeMap<Vec<u32>, Index<Accessor>>,
    samplers: Vec<Sampler>,
    channels: Vec<Channel>,
}

impl ConvertedAnimation {
    /// The animation `name`, `<hierarchy>.<animation>` as a look names it, as `Get_HAnim` finds
    /// it: in the file of the name's part past its first `.`, its hierarchy in that file or in
    /// the file of its own name, found through `assets`; the animation is taken from its file,
    /// as each converts once.
    pub(crate) fn of(name: &str, assets: &mut ModelAssets<'_>) -> Result<Self, ZeroHourError> {
        let name = name.to_ascii_lowercase();
        let Some((_, stem)) = name.split_once('.') else {
            return Ok(Self::Dropped(AnimationDropCause::Missing));
        };
        assets.load(stem)?;
        let Some(animation) = assets.take_animation(stem, &name) else {
            return Ok(Self::Dropped(AnimationDropCause::Missing));
        };
        let own = assets
            .file(stem)
            .and_then(|file| file.hierarchy(&animation.hierarchy));
        if own.is_none() {
            assets.load(&animation.hierarchy)?;
        }
        let Some(hierarchy) = [stem, animation.hierarchy.as_str()]
            .into_iter()
            .find_map(|file| assets.file(file)?.hierarchy(&animation.hierarchy))
        else {
            return Ok(Self::Dropped(AnimationDropCause::NoHierarchy));
        };
        if hierarchy.pivots.len() < 2 {
            return Ok(Self::Dropped(AnimationDropCause::NoPivot));
        }
        let ends = AnimationEnds::of(&animation, hierarchy);
        let glb = AnimationBuilder::new(&animation).build(hierarchy);
        Ok(Self::Glb {
            glb: Box::new(glb),
            ends,
        })
    }
}

impl<'a> AnimationBuilder<'a> {
    /// The builder of `animation`'s glTF, of nothing yet.
    fn new(animation: &'a W3dAnimation) -> Self {
        Self {
            animation,
            glb: GlbBuilder::new(),
            times: BTreeMap::new(),
            samplers: Vec::new(),
            channels: Vec::new(),
        }
    }

    /// The glTF of the animation on `hierarchy`, its own (09-zero-hour.md, An animation is a
    /// glTF file of its own): the hierarchy's pivot nodes, then for each pivot past the root, which
    /// `Anim_Update` places by its parent's alone, a track of its moves and one of its turns,
    /// each a key at each frame its channels' keys fall on and at its last frame; a pivot of no
    /// channel, a key of its pose at its first frame and at its last. A key's pose is
    /// `Anim_Update`'s, its pivot's pose, then the channels' move, then their turn, on glTF's
    /// axes; each pivot's visibility is in the animation's extras.
    fn build(mut self, hierarchy: &Hierarchy) -> GlbBuilder {
        let animation = self.animation;
        let PivotNodes { nodes } = PivotNodes::of(hierarchy, &mut self.glb);
        let still = PivotChannels::default();
        let mut hidden = Vec::new();
        for (at, pivot) in hierarchy.pivots.iter().enumerate().skip(1) {
            let channels = animation.pivots.get(&at).unwrap_or(&still);
            self.moves(pivot, channels, nodes[at]);
            self.turns(pivot, channels, nodes[at]);
            if let Some(visibility) = &channels.visibility {
                let runs = visibility.hidden_runs(animation.frames);
                if !runs.is_empty() {
                    let node = u32::try_from(nodes[at].value()).expect("a node's index fits u32");
                    hidden.push(HiddenFrames { node, runs });
                }
            }
        }
        let extras = AnimationExtras {
            frames: animation.frames,
            rate: animation.rate,
            hidden,
        };
        let value = Json::convert(&extras).expect("an animation's extras are a JSON object");
        let extensions = AnimationExtensions {
            others: [(AnimationExtras::EXTENSION.to_owned(), value)]
                .into_iter()
                .collect(),
        };
        self.glb.animation(Animation {
            extensions: Some(extensions),
            extras: Extras::default(),
            channels: self.channels,
            name: Some(animation.name.clone()),
            samplers: self.samplers,
        });
        self.glb
    }

    /// The track of `pivot`'s moves, of `channels`, on `node`.
    fn moves(&mut self, pivot: &Pivot, channels: &PivotChannels, node: Index<Node>) {
        let frames = self.animation.frames;
        let mut key_frames = BTreeSet::from([0, frames - 1]);
        for keys in channels.moves.iter().flatten() {
            key_frames.extend(keys.key_frames(frames));
        }
        let key_frames: Vec<u32> = key_frames.into_iter().collect();
        let moves: Vec<[f32; 3]> = key_frames
            .iter()
            .map(|&frame| axes::y_up(moved(pivot, channels, frame).to_array()))
            .collect();
        let output = self.glb.moves(&moves);
        self.track(key_frames, output, target(node, Property::Translation));
    }

    /// The track of `pivot`'s turns, of `channels`, on `node`.
    fn turns(&mut self, pivot: &Pivot, channels: &PivotChannels, node: Index<Node>) {
        let frames = self.animation.frames;
        let key_frames = channels.turn.as_ref().map_or_else(
            || BTreeSet::from([0, frames - 1]).into_iter().collect(),
            |keys| keys.key_frames(frames),
        );
        let turns: Vec<[f32; 4]> = key_frames
            .iter()
            .map(|&frame| axes::y_up_rotation(turned(pivot, channels, frame).to_array()))
            .collect();
        let output = self.glb.turns(&turns);
        self.track(key_frames, output, target(node, Property::Rotation));
    }

    /// A linear track of `target`, its values `output` at `key_frames`, whose times it shares
    /// with each track of the same frames.
    fn track(&mut self, key_frames: Vec<u32>, output: Index<Accessor>, target: Target) {
        let rate = self.animation.rate as f32;
        let input = *self.times.entry(key_frames).or_insert_with_key(|frames| {
            let times: Vec<f32> = frames.iter().map(|&frame| frame as f32 / rate).collect();
            self.glb.times(&times)
        });
        let sampler = Index::new(u32::try_from(self.samplers.len()).expect("samplers fit u32"));
        self.samplers.push(Sampler {
            extensions: None,
            extras: Extras::default(),
            input,
            interpolation: Checked::Valid(Interpolation::Linear),
            output,
        });
        self.channels.push(Channel {
            sampler,
            target,
            extensions: None,
            extras: Extras::default(),
        });
    }
}

/// The target of `node`'s `property`.
impl AnimationEnds {
    /// The poses of `hierarchy`'s pivots at `animation`'s first frame and at its last: the
    /// root's own, which `Anim_Update` never moves, and each other's moved and turned by its
    /// channels.
    fn of(animation: &W3dAnimation, hierarchy: &Hierarchy) -> Self {
        let still = PivotChannels::default();
        let at = |frame: u32| -> Vec<RigidPose> {
            let pivots = hierarchy.pivots.iter().enumerate();
            pivots
                .map(|(at, pivot)| {
                    let channels = animation.pivots.get(&at).filter(|_| at > 0);
                    let channels = channels.unwrap_or(&still);
                    RigidPose {
                        translation: moved(pivot, channels, frame),
                        rotation: turned(pivot, channels, frame),
                    }
                })
                .collect()
        };
        Self {
            first: at(0),
            last: at(animation.frames - 1),
        }
    }
}

/// `pivot`'s place in its parent's frame at `frame`, as `Anim_Update` moves it: its pose's, then
/// `channels`' move along its own axes.
fn moved(pivot: &Pivot, channels: &PivotChannels, frame: u32) -> Vec3 {
    let pose = pivot.pose();
    let along = channels
        .moves
        .each_ref()
        .map(|keys| keys.as_ref().map_or(0.0, |keys| keys.move_at(frame)));
    pose.translation + pose.rotation.rotate(Vec3::from_array(along))
}

/// `pivot`'s turn in its parent's frame at `frame`, a key of `channels`' turn or past its last:
/// its pose's, then the channels' turn.
fn turned(pivot: &Pivot, channels: &PivotChannels, frame: u32) -> Quaternion {
    let turn = channels.turn.as_ref().map_or(Quaternion::IDENTITY, |keys| {
        Quaternion::from_array(keys.turn_at(frame))
    });
    pivot.pose().rotation * turn
}

fn target(node: Index<Node>, property: Property) -> Target {
    Target {
        extensions: None,
        extras: Extras::default(),
        node,
        path: Checked::Valid(property),
    }
}

#[cfg(test)]
pub(crate) mod samples {
    use crate::zero_hour::format::w3d::hierarchy::{Hierarchy, Pivot};
    use crate::zero_hour::format::w3d::w3d_animation::samples::{channel, raw};

    /// The fixture install's animation files: `Tank_Aim.w3d`, the tank's turret a half turn about
    /// `z` at its second frame of two; `Ghost_Walk.w3d`, of a hierarchy no file holds; and
    /// `Pole_Sway.w3d`, of its own hierarchy of a root alone.
    pub(crate) fn animation_files() -> [Vec<u8>; 3] {
        let turns = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
        let aim = raw("TANK_AIM", "TANK", 2, &[channel(1, 6, 4, 0, &turns)]);
        let walk = raw("GHOST_WALK", "GHOST", 1, &[]);
        let pole = Hierarchy {
            name: "POLE".to_owned(),
            pivots: vec![Pivot {
                name: "ROOTTRANSFORM".to_owned(),
                parent: None,
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
            }],
        };
        let sway = [pole.chunk(), raw("POLE_SWAY", "POLE", 1, &[])].concat();
        [aim, walk, sway]
    }
}

#[cfg(test)]
mod tests;
