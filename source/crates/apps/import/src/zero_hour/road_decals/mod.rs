use bevy_math::{Vec2, Vec3};
use campfire_client_files::{DecalVertex, TerrainDecals};

use crate::zero_hour::convex_clip::ConvexClip;
use crate::zero_hour::road_drop_cause::RoadDropCause;
use crate::zero_hour::road_mesh::RoadMesh;
use crate::zero_hour::terrain_atlas::TexturePlace;

/// A map's road decals as the terrain file holds them, in drawing order (02-engine-core.md, A decal's
/// texture joins the terrain's): each piece's triangles split where their coordinates cross a
/// whole repeat of their type's texture, so each part samples one copy of it, at its place in the
/// terrain's texture, and each point on the engine's axes.
#[derive(Debug, Default)]
pub(crate) struct RoadDecals {
    decals: TerrainDecals,
    /// A triangle clipped to the repeat in turn.
    polygon: ConvexClip<Corner>,
}

/// A corner of a polygon being clipped: its point on the game's axes and its coordinates in
/// repeats of its texture.
/// Where a run of decals ends: how many vertices and triangles they held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecalsMark {
    vertices: usize,
    triangles: usize,
}

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
    /// The most repeats of its texture a piece's triangle spans along either axis, and the
    /// farthest from 0 its coordinates lie, in repeats: the game's roads, 8 to 35 wide on maps of
    /// a few thousand units, span a few hundred, and a coordinate past 2²⁰ steps by whole repeats
    /// no more exactly in an `f32`.
    const MOST_REPEATS: f32 = 65_536.0;
    const FARTHEST: f32 = 1_048_576.0;

    /// Where the decals end, which `truncate` cuts back to.
    pub(crate) const fn mark(&self) -> DecalsMark {
        DecalsMark {
            vertices: self.decals.vertices.len(),
            triangles: self.decals.triangles.len(),
        }
    }

    /// Drops every decal added after `mark`.
    pub(crate) fn truncate(&mut self, mark: DecalsMark) {
        self.decals.vertices.truncate(mark.vertices);
        self.decals.triangles.truncate(mark.triangles);
    }

    /// Adds each triangle of `mesh`, a piece of a road whose texture is at `place`, turned
    /// counter-clockwise seen from above: for each repeat of the texture its coordinates cross, the
    /// triangle clipped to that repeat, a part of no area dropped, as a fan of its corners, each
    /// corner's coordinates its share of the repeat over the copy. An error, with what it added
    /// kept, for a triangle past `MOST_REPEATS` or `FARTHEST`.
    pub(crate) fn add(
        &mut self,
        mesh: &RoadMesh,
        place: TexturePlace,
    ) -> Result<(), RoadDropCause> {
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
            let near = |at: Vec2| at.abs().max_element() <= Self::FARTHEST;
            let finite = corners.iter().all(|corner| corner.uv.is_finite());
            let span = last - first;
            if !(finite && near(first) && near(last) && span.max_element() <= Self::MOST_REPEATS) {
                return Err(RoadDropCause::Repeats);
            }
            let whole = |at: f32| at as i32;
            for v in whole(first.y)..=whole(last.y) {
                for u in whole(first.x)..=whole(last.x) {
                    let repeat = Vec2::new(u as f32, v as f32);
                    self.clip(&corners, repeat);
                    self.fan(repeat, corner, size);
                }
            }
        }
        Ok(())
    }

    /// The decals, as a terrain file holds them.
    pub(crate) fn into_decals(self) -> TerrainDecals {
        self.decals
    }

    /// Clips `triangle` into `polygon` to the repeat from `repeat` to `repeat + 1` on both axes.
    fn clip(&mut self, triangle: &[Corner; 3], repeat: Vec2) {
        self.polygon.start(triangle);
        for axis in 0..2 {
            for (at, above) in [(repeat[axis], true), (repeat[axis] + 1.0, false)] {
                Bound { axis, at, above }.clip(&mut self.polygon);
            }
        }
    }

    /// Adds the clipped polygon of the repeat at `repeat`, its texture's copy at `corner` and of
    /// `size` texels, as a fan of triangles from its first corner; none when it has no area.
    fn fan(&mut self, repeat: Vec2, corner: Vec2, size: Vec2) {
        if !self.polygon.has_area(|corner| corner.uv) {
            return;
        }
        let polygon = self.polygon.corners();
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
    /// Clips `polygon` to the kept side, each crossing's coordinate on the bound's axis the
    /// bound's exactly.
    fn clip(self, polygon: &mut ConvexClip<Corner>) {
        polygon.clip(
            |corner| {
                let at = corner.uv[self.axis];
                if self.above {
                    at - self.at
                } else {
                    self.at - at
                }
            },
            |from, to, share| {
                let mut uv = from.uv + (to.uv - from.uv) * share;
                uv[self.axis] = self.at;
                Corner {
                    point: from.point + (to.point - from.point) * share,
                    uv,
                }
            },
        );
    }
}

#[cfg(test)]
mod tests;
