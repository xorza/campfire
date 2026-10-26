use bevy::color::{ColorToComponents, LinearRgba, Srgba};
use bevy::math::Vec3;
use campfire_client_files::Blend;

use crate::view::effects::effect_book::{EffectBook, GroupId};
use crate::view::effects::particle_run::ParticleRun;

/// The quads the particles alive draw as, one list for each of a book's draw groups
/// (effects.md, Particles run on the CPU and draw as billboards of the one material): each
/// particle a square of its size about its place, turned by its angle, facing the camera or lying
/// on the ground, its texture whole across it, tinted by its color and its alpha. Its lists are
/// the scratch of each frame in turn.
#[derive(Debug, Default)]
pub(crate) struct ParticleQuads {
    groups: Vec<GroupQuads>,
}

/// A draw group's quads: four vertices each, of their points, their texture coordinates and
/// their colors, linear, and two triangles each, counter-clockwise seen from their front.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct GroupQuads {
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) uvs: Vec<[f32; 2]>,
    pub(crate) colors: Vec<[f32; 4]>,
    pub(crate) indices: Vec<u32>,
}

/// The directions of the view: its right and its up, which a quad facing the camera spans.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewAxes {
    pub(crate) right: Vec3,
    pub(crate) up: Vec3,
}

/// A quad's corners in its own plane, from its top left counter-clockwise, and their texture
/// coordinates (`PointGroupClass`'s quad).
const CORNERS: [[f32; 2]; 4] = [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]];
const UVS: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
/// The game's ground plane, its `x` and its `y`, on the engine's axes.
const GROUND: ViewAxes = ViewAxes {
    right: Vec3::X,
    up: Vec3::NEG_Z,
};

impl ParticleQuads {
    /// Fills its lists with the quads of `run`'s particles, of `book`'s groups, seen along
    /// `view`. A particle of an alpha test draws its texture alone, as the game's shader of one
    /// draws no tint.
    pub(crate) fn build(&mut self, book: &EffectBook, run: &ParticleRun, view: ViewAxes) {
        let groups = book.groups();
        self.groups.resize_with(groups.len(), GroupQuads::default);
        for group in &mut self.groups {
            group.positions.clear();
            group.uvs.clear();
            group.colors.clear();
            group.indices.clear();
        }
        for particle in run.particles() {
            let axes = if particle.ground_aligned {
                GROUND
            } else {
                view
            };
            let [sin, cos] = [particle.angle.sin(), particle.angle.cos()];
            let color = if let Blend::Mask { .. } = groups[particle.group.index()].blend {
                [1.0; 4]
            } else {
                let [red, green, blue] = particle.color.to_array();
                LinearRgba::from(Srgba::new(red, green, blue, particle.alpha)).to_f32_array()
            };
            let quads = &mut self.groups[particle.group.index()];
            let first = u32::try_from(quads.positions.len()).expect("a frame's quads fit u32");
            for ([x, y], uv) in CORNERS.into_iter().zip(UVS) {
                let [x, y] = [x * cos - y * sin, x * sin + y * cos];
                let corner = particle.position + (axes.right * x + axes.up * y) * particle.size;
                quads.positions.push(corner.to_array());
                quads.uvs.push(uv);
                quads.colors.push(color);
            }
            quads
                .indices
                .extend([0, 1, 2, 0, 2, 3].map(|at| first + at));
        }
    }

    /// The quads of the group `id`.
    pub(crate) fn group(&self, id: GroupId) -> &GroupQuads {
        &self.groups[id.index()]
    }
}

#[cfg(test)]
mod tests;
