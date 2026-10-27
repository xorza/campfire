use bevy::ecs::entity::Entity;
use bevy::math::{Affine3A, Vec3};
use campfire_client_files::TreadDust;

use crate::view::effects::particle_run::{ParticleRun, SystemKey};

/// The dust each drawn unit's treads throw (02-engine-core.md, A tread throws dust while it
/// drives): its tread's systems on its drawing, which run while it drives along the ground, scaled
/// by its speed as the effects file's dust says, and stop as it stands.
#[derive(Debug)]
pub(crate) struct TreadRun {
    dust: TreadDust,
    treads: Vec<Tread>,
}

/// A drawing's treads: its root, and the systems they run.
#[derive(Debug)]
struct Tread {
    root: Entity,
    systems: Vec<SystemKey>,
}

/// A drawing as its treads follow it: where it stands, and the velocity it moves at, in metres a
/// second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Driven {
    pub(crate) at: Affine3A,
    pub(crate) velocity: Vec3,
}

impl TreadRun {
    pub(crate) const fn new(dust: TreadDust) -> Self {
        Self {
            dust,
            treads: Vec::new(),
        }
    }

    /// Gives the drawing `root` the treads of `systems`, which its next drive places, runs or
    /// stops.
    pub(crate) fn add(&mut self, root: Entity, systems: impl IntoIterator<Item = SystemKey>) {
        let systems = systems.into_iter().collect();
        self.treads.push(Tread { root, systems });
    }

    /// Places each drawing's systems where `place` gives it stands, each running while its speed
    /// along the ground passes the dust's, scaled across, up and in count by it, as
    /// `W3DTankDraw::doDrawModule` scales them; a drawing `place` gives none of went, and its
    /// systems with it, its particles living out their lives.
    pub(crate) fn drive(
        &mut self,
        run: &mut ParticleRun,
        place: impl Fn(Entity) -> Option<Driven>,
    ) {
        let dust = self.dust;
        self.treads.retain(|tread| {
            let Some(driven) = place(tread.root) else {
                for &system in &tread.systems {
                    run.destroy(system);
                }
                return false;
            };
            let speed = Vec3::new(driven.velocity.x, 0.0, driven.velocity.z).length();
            let across = dust.across.at(speed);
            let velocity = Vec3::new(across, dust.rise.at(speed), across);
            let count = dust.count.at(speed);
            for &system in &tread.systems {
                run.place(system, driven.at);
                if speed > dust.moving {
                    run.resume(system);
                } else {
                    run.stop(system);
                }
                run.scale(system, velocity, count);
            }
            true
        });
    }
}

#[cfg(test)]
mod tests;
