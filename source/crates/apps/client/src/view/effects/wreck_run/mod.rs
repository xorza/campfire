use std::f32::consts::TAU;

use bevy::asset::AssetPath;
use bevy::ecs::entity::Entity;
use bevy::gltf::GltfAssetLabel;
use bevy::math::{Affine3A, EulerRot, Quat, Vec3};
use bevy::transform::components::Transform;
use campfire_client_files::{Wreck, WreckGround, WreckLaunch, WreckList};
use campfire_math::SplitMix64;
use campfire_values::PackageIndex;

use crate::view::float_num::FloatNum;
use crate::view::package_source::PackageAssets;

/// The wrecks of the match's deaths, the client's alone (02-engine-core.md, A wreck is the
/// client's): each piece thrown from its unit's drawing, stepped at the effects' step by their
/// gravity, landing on the drawn ground, resting, sinking and gone (02-engine-core.md, Wrecks move
/// by the client's step); and the client's own random sequence its throws draw from, as nothing of
/// them is state.
#[derive(Debug)]
pub(crate) struct WreckRun {
    pieces: Vec<WreckPiece>,
    rng: SplitMix64,
}

/// A look's wreck as the client throws it: the scenes of its models, one drawn for each piece,
/// its count, its offset from its unit's drawing, its launch, its spin and its end, each in the
/// client's floats.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WreckPlay {
    pub(crate) scenes: Vec<AssetPath<'static>>,
    pub(crate) count: u8,
    pub(crate) offset: Vec3,
    pub(crate) launch: WreckLaunch,
    pub(crate) spin: f32,
    pub(crate) end: WreckEnd,
}

/// How a piece ends: a hulk after its `life`, debris as it rests; then it sinks at `rate` after
/// `delay`, and goes `gone` after its end, each in seconds and metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WreckEnd {
    pub(crate) life: Option<f32>,
    pub(crate) delay: f32,
    pub(crate) rate: f32,
    pub(crate) gone: f32,
}

/// A piece in the world: its entity, its place, velocity, turn and spin, its age and where it
/// stands in its life.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WreckPiece {
    pub(crate) entity: Entity,
    pub(crate) at: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) turn: Quat,
    pub(crate) spin: Vec3,
    age: f32,
    stage: PieceStage,
    end: WreckEnd,
}

/// Where a piece stands in its life: moving, resting on the ground, or ended since its age then,
/// sinking after its delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PieceStage {
    Moving,
    Resting,
    Ended { since: f32 },
}

impl WreckRun {
    pub(crate) const fn new(seed: u64) -> Self {
        Self {
            pieces: Vec::new(),
            rng: SplitMix64::new(seed),
        }
    }

    /// Throws one of `lists`, drawn at random, as `throw` throws it; none of no list.
    pub(crate) fn throw_one(
        &mut self,
        lists: &[Vec<WreckPlay>],
        at: Affine3A,
        ground: &impl Fn(f32, f32) -> f32,
        spawn: impl FnMut(&AssetPath<'static>, Transform) -> Entity,
    ) {
        let count = u64::try_from(lists.len()).expect("a count fits u64");
        if count == 0 {
            return;
        }
        let at_list = usize::try_from(self.rng.below(count)).expect("an index below the count");
        self.throw(&lists[at_list], at, ground, spawn);
    }

    /// Throws the pieces of `plays` from a drawing at `at`, over a ground of `ground`'s heights,
    /// each spawned by `spawn` of its scene where it starts.
    pub(crate) fn throw(
        &mut self,
        plays: &[WreckPlay],
        at: Affine3A,
        ground: &impl Fn(f32, f32) -> f32,
        mut spawn: impl FnMut(&AssetPath<'static>, Transform) -> Entity,
    ) {
        let (_, rotation, place) = at.to_scale_rotation_translation();
        for play in plays {
            for _ in 0..play.count {
                let Some(scene) = self.pick(&play.scenes) else {
                    continue;
                };
                let mut start = place + rotation * play.offset;
                let mut turn = rotation;
                let velocity = match play.launch {
                    WreckLaunch::Still => Vec3::ZERO,
                    WreckLaunch::Grounded => {
                        start.y = ground(start.x, start.z);
                        turn = Quat::from_rotation_y(self.uniform(0.0, TAU));
                        Vec3::ZERO
                    }
                    WreckLaunch::Out { across } => {
                        start.y = ground(start.x, start.z);
                        let across = across.float();
                        Vec3::new(
                            self.uniform(-across, across),
                            0.0,
                            self.uniform(-across, across),
                        )
                    }
                    WreckLaunch::Fling { across, up } => {
                        let across = across.float();
                        Vec3::new(
                            self.uniform(-across, across),
                            self.uniform(up[0].float(), up[1].float()),
                            self.uniform(-across, across),
                        )
                    }
                    WreckLaunch::Force { speed, pitch } => {
                        let angle = self.uniform(0.0, TAU);
                        let pitch = self.uniform(pitch[0].float(), pitch[1].float());
                        let speed = self.uniform(speed[0].float(), speed[1].float());
                        let flat = speed * pitch.cos();
                        Vec3::new(flat * angle.cos(), speed * pitch.sin(), flat * angle.sin())
                    }
                };
                let spin = Vec3::new(
                    self.uniform(-play.spin, play.spin),
                    self.uniform(-play.spin, play.spin),
                    self.uniform(-play.spin, play.spin),
                );
                let entity = spawn(
                    &scene,
                    Transform::from_translation(start).with_rotation(turn),
                );
                self.pieces.push(WreckPiece {
                    entity,
                    at: start,
                    velocity,
                    turn,
                    spin,
                    age: 0.0,
                    stage: PieceStage::Moving,
                    end: play.end,
                });
            }
        }
    }

    /// Steps each piece `step` seconds by `physics` over a ground of `ground`'s heights, and puts
    /// the entity of each that went into `gone`.
    pub(crate) fn step(
        &mut self,
        step: f32,
        physics: WreckGround,
        ground: &impl Fn(f32, f32) -> f32,
        gone: &mut Vec<Entity>,
    ) {
        self.pieces.retain_mut(|piece| {
            let stays = piece.step(step, physics, ground(piece.at.x, piece.at.z));
            if !stays {
                gone.push(piece.entity);
            }
            stays
        });
    }

    pub(crate) fn pieces(&self) -> &[WreckPiece] {
        &self.pieces
    }

    /// One of `scenes`, drawn at random; none of none.
    fn pick(&mut self, scenes: &[AssetPath<'static>]) -> Option<AssetPath<'static>> {
        let count = u64::try_from(scenes.len()).expect("a count fits u64");
        let at = (count > 0).then(|| self.rng.below(count))?;
        scenes
            .get(usize::try_from(at).expect("an index below the count"))
            .cloned()
    }

    /// A value from `low` to `high`, uniformly, from 24 bits of a draw.
    #[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32 whole")]
    fn uniform(&mut self, low: f32, high: f32) -> f32 {
        let share = (self.rng.next_u64() >> 40) as f32 / (1_u32 << 24) as f32;
        low + (high - low) * share
    }
}

impl WreckPlay {
    /// The client's plays of each of `lists`, of the models of the package `package`.
    pub(crate) fn lists(lists: &[WreckList], package: PackageIndex) -> Vec<Vec<Self>> {
        let list = |list: &WreckList| {
            list.0
                .iter()
                .map(|wreck| Self::of(wreck, package))
                .collect()
        };
        lists.iter().map(list).collect()
    }

    /// The client's play of `wreck`, of the models of the package `package`.
    pub(crate) fn of(wreck: &Wreck, package: PackageIndex) -> Self {
        let [x, y, z] = wreck.offset.map(FloatNum::float);
        Self {
            scenes: (wreck.models.iter())
                .map(|model| {
                    GltfAssetLabel::Scene(0).from_asset(PackageAssets::asset_path(package, model))
                })
                .collect(),
            count: wreck.count,
            offset: Vec3::new(x, y, z),
            launch: wreck.launch,
            spin: wreck.spin.float(),
            end: WreckEnd {
                life: wreck.sink.life.map(FloatNum::float),
                delay: wreck.sink.delay.float(),
                rate: wreck.sink.rate.float(),
                gone: wreck.sink.gone.float(),
            },
        }
    }
}

impl WreckPiece {
    /// Steps it `step` seconds by `physics` over the ground `ground` high: it falls, turns and
    /// moves; on the ground, it lands, the ground taking the stiffness's share of its speed
    /// down, set upright about its yaw, and the friction's share of its speed along the ground
    /// and of its spin each step, and rests below the rest's speed; a hulk ends after its life
    /// and debris as it rests, then sinks after its delay and goes after its gone. Whether it
    /// stays.
    pub(crate) fn step(&mut self, step: f32, physics: WreckGround, ground: f32) -> bool {
        self.age += step;
        if let (PieceStage::Moving | PieceStage::Resting, Some(life)) = (self.stage, self.end.life)
            && self.age >= life
        {
            self.stage = PieceStage::Ended { since: self.age };
        }
        match self.stage {
            PieceStage::Moving => self.fall(step, physics, ground),
            PieceStage::Resting => {}
            PieceStage::Ended { since } => {
                let ended = self.age - since;
                if ended >= self.end.gone {
                    return false;
                }
                if ended >= self.end.delay {
                    self.at.y -= self.end.rate * step;
                } else {
                    self.fall(step, physics, ground);
                }
            }
        }
        true
    }

    /// Moves it a step by its velocity and spin under gravity, and on the ground lands, slows and
    /// rests it.
    fn fall(&mut self, step: f32, physics: WreckGround, ground: f32) {
        if self.velocity == Vec3::ZERO && self.spin == Vec3::ZERO && self.at.y <= ground {
            return;
        }
        self.velocity.y += physics.gravity * step;
        self.at += self.velocity * step;
        self.turn = Quat::from_scaled_axis(self.spin * step) * self.turn;
        if self.at.y > ground {
            return;
        }
        self.at.y = ground;
        if self.velocity.y < 0.0 {
            self.velocity.y *= 1.0 - physics.stiffness;
            let (yaw, _, _) = self.turn.to_euler(EulerRot::YXZ);
            self.turn = Quat::from_rotation_y(yaw);
        }
        let keep = 1.0 - physics.friction;
        self.velocity.x *= keep;
        self.velocity.z *= keep;
        self.spin *= keep;
        let still = |speed: f32| speed.abs() < physics.rest;
        if still(self.velocity.x) && still(self.velocity.y) && still(self.velocity.z) {
            self.velocity = Vec3::ZERO;
            self.spin = Vec3::ZERO;
            if self.stage == PieceStage::Moving {
                self.stage = if self.end.life.is_some() {
                    PieceStage::Resting
                } else {
                    PieceStage::Ended { since: self.age }
                };
            }
        }
    }

    /// Where it stands and how it turns.
    pub(crate) const fn transform(&self) -> Transform {
        Transform::from_translation(self.at).with_rotation(self.turn)
    }
}

#[cfg(test)]
mod tests;
