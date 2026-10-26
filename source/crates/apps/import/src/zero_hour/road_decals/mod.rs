use bevy_math::{Vec2, Vec3};
use campfire_client_files::{DecalVertex, TerrainDecals};

use crate::zero_hour::road_mesh::RoadMesh;
use crate::zero_hour::terrain_atlas::TexturePlace;

/// A map's road decals as the terrain file holds them, in drawing order (roads.md, A decal's
/// texture joins the terrain's): each piece's triangles split where their coordinates cross a
/// whole repeat of their type's texture, so each part samples one copy of it, at its place in the
/// terrain's texture, and each point on the engine's axes.
#[derive(Debug, Default)]
pub(crate) struct RoadDecals {
    decals: TerrainDecals,
    /// The polygon being clipped, and the one each clip writes, swapped after it.
    polygon: Vec<Corner>,
    clipped: Vec<Corner>,
}

/// A corner of a polygon being clipped: its point on the game's axes and its coordinates in
/// repeats of its texture.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Corner {
    point: Vec3,
    uv: Vec2,
}

/// A side of a repeat a polygon is clipped to: its axis of the coordinates, its bound, and
/// whether the side kept lies above it.
#[derive(Debug, Clone, Copy)]
struct Bound {
    axis: usize,
    at: f32,
    above: bool,
}

impl RoadDecals {
    /// Adds each triangle of `mesh`, a piece of a road whose texture is at `place`, turned
    /// counter-clockwise seen from above: for each repeat of the texture its coordinates cross, the
    /// triangle clipped to that repeat, a part of no area dropped, as a fan of its corners, each
    /// corner's coordinates its share of the repeat over the copy.
    pub(crate) fn add(&mut self, mesh: &RoadMesh, place: TexturePlace) {
        let corner = place.corner.as_vec2();
        let size = place.size.as_vec2();
        for triangle in &mesh.triangles {
            let mut corners = triangle.map(|at| Corner {
                point: mesh.points[at as usize],
                uv: mesh.uvs[at as usize],
            });
            // A flipped segment's corners swap its sides, so the game winds its triangles the
            // other way; a decal faces up whichever way, as nothing draws its back.
            let [a, b, c] = corners.map(|corner| corner.point.truncate());
            if (b - a).perp_dot(c - a) < 0.0 {
                corners.swap(1, 2);
            }
            let low = corners
                .iter()
                .fold(Vec2::INFINITY, |low, at| low.min(at.uv));
            let high = corners
                .iter()
                .fold(Vec2::NEG_INFINITY, |high, at| high.max(at.uv));
            let first = low.floor();
            let last = (high.ceil() - 1.0).max(first);
            let mut v = first.y;
            while v <= last.y {
                let mut u = first.x;
                while u <= last.x {
                    let repeat = Vec2::new(u, v);
                    self.clip(&corners, repeat);
                    self.fan(repeat, corner, size);
                    u += 1.0;
                }
                v += 1.0;
            }
        }
    }

    /// The decals, as a terrain file holds them.
    pub(crate) fn into_decals(self) -> TerrainDecals {
        self.decals
    }

    /// Clips `triangle` into `polygon` to the repeat from `repeat` to `repeat + 1` on both axes.
    fn clip(&mut self, triangle: &[Corner; 3], repeat: Vec2) {
        self.polygon.clear();
        self.polygon.extend_from_slice(triangle);
        let bounds = [0, 1].into_iter().flat_map(|axis| {
            [(repeat[axis], true), (repeat[axis] + 1.0, false)].map(|(at, above)| Bound {
                axis,
                at,
                above,
            })
        });
        for bound in bounds {
            self.clipped.clear();
            let count = self.polygon.len();
            for at in 0..count {
                let from = self.polygon[at];
                let to = self.polygon[(at + 1) % count];
                let [inside_from, inside_to] = [from, to].map(|corner| bound.holds(corner));
                if inside_from {
                    self.clipped.push(from);
                }
                if inside_from != inside_to {
                    self.clipped.push(bound.cross(from, to));
                }
            }
            std::mem::swap(&mut self.polygon, &mut self.clipped);
        }
    }

    /// Adds the clipped polygon of the repeat at `repeat`, its texture's copy at `corner` and of
    /// `size` texels, as a fan of triangles from its first corner; none when it has no area.
    fn fan(&mut self, repeat: Vec2, corner: Vec2, size: Vec2) {
        let polygon = &self.polygon;
        let area: f32 = (0..polygon.len())
            .map(|at| {
                let [from, to] = [polygon[at].uv, polygon[(at + 1) % polygon.len()].uv];
                from.perp_dot(to)
            })
            .sum();
        if polygon.len() < 3 || area == 0.0 {
            return;
        }
        let first = u32::try_from(self.decals.vertices.len()).expect("a map's decals fit u32");
        self.decals
            .vertices
            .extend(polygon.iter().map(|at| DecalVertex {
                position: Vec3::new(at.point.x, at.point.z, -at.point.y),
                uv: corner + (at.uv - repeat) * size,
            }));
        let count = u32::try_from(polygon.len()).expect("a clipped triangle's corners fit u32");
        self.decals
            .triangles
            .extend((1..count - 1).map(|at| [first, first + at, first + at + 1]));
    }
}

impl Bound {
    /// Whether `corner` lies on the kept side, or on the bound.
    fn holds(self, corner: Corner) -> bool {
        let at = corner.uv[self.axis];
        if self.above {
            at >= self.at
        } else {
            at <= self.at
        }
    }

    /// The corner where the edge from `from` to `to`, one on each side, crosses the bound, its
    /// coordinate on the bound's axis the bound's exactly.
    fn cross(self, from: Corner, to: Corner) -> Corner {
        let [start, end] = [from.uv[self.axis], to.uv[self.axis]];
        let share = (self.at - start) / (end - start);
        let mut uv = from.uv + (to.uv - from.uv) * share;
        uv[self.axis] = self.at;
        Corner {
            point: from.point + (to.point - from.point) * share,
            uv,
        }
    }
}

#[cfg(test)]
mod tests;
