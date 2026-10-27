use std::collections::BTreeMap;

use bevy::ecs::entity::Entity;
use bevy::math::{Affine3A, Quat, Vec3};
use bevy::transform::components::Transform;
use campfire_client_files::{DeathFall, FallStage};
use campfire_values::{PackageIndex, UnitType};

use crate::view::effects::effect_book::{EffectBook, ListId};
use crate::view::effects::wreck_run::WreckPlay;
use crate::view::float_num::FloatNum;

/// The aircraft that fall as wrecks, the client's alone (wrecks.md, An aircraft's death falls as
/// a wreck): each drawing torn off its unit as its death started, falling at its type's share of
/// the effects' gravity, rolling or spiralling, which plays its hit as it meets the drawn ground
/// and its blow-up its delay after, then goes.
#[derive(Debug, Default)]
pub(crate) struct FallRun {
    falls: Vec<Falling>,
}

/// A type's fall as the client plays it: its share of gravity, its roll, its spiral, its start,
/// its hit, its blow-up after its delay, and a jet's death on the ground, each moment's lists of
/// those the book holds and its wrecks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FallPlay {
    pub(crate) gravity: f32,
    pub(crate) roll: f32,
    pub(crate) spiral: Option<SpiralPlay>,
    pub(crate) start: MomentPlay,
    pub(crate) hit: MomentPlay,
    pub(crate) blow_up_after: f32,
    pub(crate) blow_up: MomentPlay,
    pub(crate) ground: Option<GroundPlay>,
}

/// A jet's death on the ground: the height above it at or under which it dies there, and what it
/// plays in place of its fall.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroundPlay {
    pub(crate) height: f32,
    pub(crate) moment: MomentPlay,
}

/// A helicopter's spiral: its turn about its path and its speed along it, and its own spin, in
/// radians and metres a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpiralPlay {
    pub(crate) turn: f32,
    pub(crate) speed: f32,
    pub(crate) spin: f32,
}

/// What a moment of a fall plays: its lists, of which it plays one drawn at random, and its
/// wrecks, of which it throws one list.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MomentPlay {
    pub(crate) lists: Vec<ListId>,
    pub(crate) wrecks: Vec<Vec<WreckPlay>>,
}

/// A drawing that falls: its root, its type, where it stands, its velocity and its turn, its age,
/// and the age it met the ground at, once it did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Falling {
    pub(crate) root: Entity,
    pub(crate) unit_type: UnitType,
    pub(crate) at: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) turn: Quat,
    age: f32,
    down: Option<f32>,
}

/// A drawing whose fall starts: its root, its type, where it stands and the velocity it moves at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FallFrom {
    pub(crate) root: Entity,
    pub(crate) unit_type: UnitType,
    pub(crate) at: Affine3A,
    pub(crate) velocity: Vec3,
}

/// What a fall's step comes to for one drawing: it hit the ground, or blew up and went, where it
/// stood.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FallEvent {
    Hit {
        root: Entity,
        unit_type: UnitType,
        at: Affine3A,
    },
    BlowUp {
        root: Entity,
        unit_type: UnitType,
        at: Affine3A,
    },
}

/// How a death with a fall starts, and what it plays as it does: it falls, playing its start, or
/// a jet on the ground plays its ground death alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FallStart<'p> {
    Falls(&'p MomentPlay),
    OnGround(&'p MomentPlay),
}

impl FallRun {
    /// Starts the fall of `from` by `play`, over ground `ground` high under it: a helicopter
    /// along its facing at its spiral's speed, a jet on at its velocity, as its locomotor keeps
    /// its speed and turns no more (`JetSlowDeathBehavior::beginSlowDeath`); a jet at most its
    /// ground death's height above the ground falls not.
    pub(crate) fn start<'p>(
        &mut self,
        from: FallFrom,
        play: &'p FallPlay,
        ground: f32,
    ) -> FallStart<'p> {
        let (_, turn, at) = from.at.to_scale_rotation_translation();
        if let Some(on_ground) = &play.ground
            && at.y - ground <= on_ground.height
        {
            return FallStart::OnGround(&on_ground.moment);
        }
        let velocity = match play.spiral {
            Some(spiral) => {
                let forward = turn * Vec3::X;
                Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero() * spiral.speed
            }
            None => from.velocity,
        };
        self.falls.push(Falling {
            root: from.root,
            unit_type: from.unit_type,
            at,
            velocity,
            turn,
            age: 0.0,
            down: None,
        });
        FallStart::Falls(&play.start)
    }

    /// Steps each falling drawing `step` seconds by its type's play of `plays`, which holds every
    /// type that falls, at `gravity` times its share, over a ground of `ground`'s heights; each hit
    /// and each blow-up into `events`, the blown up let go.
    pub(crate) fn step(
        &mut self,
        step: f32,
        gravity: f32,
        plays: &BTreeMap<UnitType, FallPlay>,
        ground: &impl Fn(f32, f32) -> f32,
        events: &mut Vec<FallEvent>,
    ) {
        self.falls.retain_mut(|fall| {
            let play = &plays[&fall.unit_type];
            let stepped = fall.step(step, gravity, play, ground(fall.at.x, fall.at.z));
            let (root, unit_type) = (fall.root, fall.unit_type);
            let at = fall.transform().compute_affine();
            match stepped {
                FallStep::Falls => true,
                FallStep::Hits => {
                    events.push(FallEvent::Hit {
                        root,
                        unit_type,
                        at,
                    });
                    true
                }
                FallStep::BlowsUp => {
                    events.push(FallEvent::BlowUp {
                        root,
                        unit_type,
                        at,
                    });
                    false
                }
            }
        });
    }

    pub(crate) fn falls(&self) -> &[Falling] {
        &self.falls
    }
}

impl FallPlay {
    /// The client's play of `fall`, its lists of those `book` holds and its wrecks of the mode's
    /// own package's models.
    pub(crate) fn of(fall: &DeathFall, book: &EffectBook) -> Self {
        let moment = |stage: &FallStage| MomentPlay {
            lists: (stage.effects.iter())
                .filter_map(|name| book.list_id(name))
                .collect(),
            wrecks: WreckPlay::lists(&stage.wrecks, PackageIndex::MODE),
        };
        Self {
            gravity: fall.gravity.float(),
            roll: fall.roll.float(),
            spiral: fall.spiral.map(|spiral| SpiralPlay {
                turn: spiral.turn.float(),
                speed: spiral.speed.float(),
                spin: spiral.spin.float(),
            }),
            start: moment(&fall.start),
            hit: moment(&fall.hit),
            blow_up_after: fall.blow_up_after.float(),
            blow_up: moment(&fall.blow_up),
            ground: fall.ground.as_ref().map(|ground| GroundPlay {
                height: ground.height.float(),
                moment: moment(&ground.stage),
            }),
        }
    }
}

impl Falling {
    /// Steps it `step` seconds by `play` at `gravity` times its share over the ground `ground`
    /// high: falling, it rolls about its facing or turns its path and spins, and meets the ground,
    /// a hit; down, it waits its blow-up's delay, a blow-up.
    pub(crate) fn step(
        &mut self,
        step: f32,
        gravity: f32,
        play: &FallPlay,
        ground: f32,
    ) -> FallStep {
        self.age += step;
        if let Some(down) = self.down {
            return if self.age - down >= play.blow_up_after {
                FallStep::BlowsUp
            } else {
                FallStep::Falls
            };
        }
        self.velocity.y += gravity * play.gravity * step;
        if let Some(spiral) = play.spiral {
            let path = Quat::from_rotation_y(spiral.turn * step);
            let flat = path * Vec3::new(self.velocity.x, 0.0, self.velocity.z);
            self.velocity = Vec3::new(flat.x, self.velocity.y, flat.z);
            self.turn = Quat::from_rotation_y(spiral.spin * step) * self.turn;
        }
        self.turn *= Quat::from_rotation_x(play.roll * step);
        self.at += self.velocity * step;
        if self.at.y > ground {
            return FallStep::Falls;
        }
        self.at.y = ground;
        self.velocity = Vec3::ZERO;
        self.down = Some(self.age);
        FallStep::Hits
    }

    /// Where it stands and how it turns.
    pub(crate) const fn transform(&self) -> Transform {
        Transform::from_translation(self.at).with_rotation(self.turn)
    }
}

/// What a step of one fall comes to: it falls on or lies waiting, it hits the ground, or it blows
/// up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallStep {
    Falls,
    Hits,
    BlowsUp,
}

#[cfg(test)]
mod tests;
