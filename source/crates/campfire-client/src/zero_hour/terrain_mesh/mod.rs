use bevy::asset::RenderAssetUsages;
use bevy::color::{LinearRgba, Srgba};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use campfire_capabilities::HeightGrid;
use campfire_package::{TerrainAtlas, TerrainLighting};

use crate::view::float_num::FloatNum;
use crate::zero_hour::terrain_cells::{Layer, TerrainCells};

/// One mesh of Zero Hour's terrain on the engine's axes, four vertices a cell, at its corners in
/// the game's order, as the game's heightmap renderer builds its cells: a sample's position is
/// its place on the map at its height, and its color the light the game's vertex lighting gives
/// it, by its normal across its neighbors on each axis, the map's edge clamping them. Each cell's
/// two triangles meet on the diagonal its layer says. Texture coordinates are shares of the
/// atlas.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TerrainMesh {
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) uvs: Vec<[f32; 2]>,
    /// Each corner's light, linear, and its alpha: an overlay's alone is not opaque.
    pub(crate) colors: Vec<[f32; 4]>,
    pub(crate) indices: Vec<u32>,
}

/// The terrain's two meshes: the ground, each cell's tile; and its overlay, each cell's blend
/// and third blend over it, in that order within each cell, as the game draws the third after.
#[derive(Debug)]
pub(crate) struct TerrainMeshes {
    pub(crate) ground: TerrainMesh,
    pub(crate) overlay: TerrainMesh,
}

/// Where the cells' samples lie and how high, on the engine's axes, as floats, and the light
/// they are drawn in.
#[derive(Debug, Clone, Copy)]
struct Placement<'a> {
    origin: [f32; 2],
    cell: f32,
    step: f32,
    rows: usize,
    lighting: &'a TerrainLighting,
}

/// The game's distance between a sample's two neighbors on an axis: two cells, whatever the
/// map's edge clamps.
const ACROSS: f32 = 20.0;

impl TerrainMeshes {
    /// The meshes of `cells` on the map `grid` in `lighting`, with `atlas`'s size.
    #[expect(
        clippy::cast_precision_loss,
        reason = "texels, cells and steps of height lie far below 2²⁴, exact in an f32"
    )]
    pub(crate) fn of(
        cells: &TerrainCells<'_>,
        grid: &HeightGrid,
        atlas: &TerrainAtlas,
        lighting: &TerrainLighting,
    ) -> TerrainMeshes {
        let [x, z] = grid.origin();
        let placement = Placement {
            origin: [x.float(), z.float()],
            cell: grid.cell().float(),
            step: grid.step().float(),
            rows: grid.rows() as usize,
            lighting,
        };
        let size = [TerrainAtlas::WIDTH as f32, atlas.height() as f32];
        let [columns, rows] = cells.drawn();
        let mut ground = TerrainMesh::default();
        let mut overlay = TerrainMesh::default();
        for y in 0..rows {
            for x in 0..columns {
                let draw = cells.cell([x, y]);
                let corners = [[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1]];
                let tile = Layer {
                    uv: draw.tile,
                    alpha: [255; 4],
                    flip: draw.blend.flip,
                };
                ground.cell(cells, placement, corners, tile, size);
                if draw.blend.alpha != [0; 4] {
                    overlay.cell(cells, placement, corners, draw.blend, size);
                }
                if let Some(extra) = draw.extra {
                    overlay.cell(cells, placement, corners, extra, size);
                }
            }
        }
        TerrainMeshes { ground, overlay }
    }
}

impl TerrainMesh {
    /// The mesh for Bevy, its vertices and indices in the main world and the render world.
    pub(crate) fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }

    /// Adds the cell whose corners are the samples `corners` with `layer`, its texels shares of
    /// `size`.
    fn cell(
        &mut self,
        cells: &TerrainCells<'_>,
        placement: Placement<'_>,
        corners: [[usize; 2]; 4],
        layer: Layer,
        size: [f32; 2],
    ) {
        let first = u32::try_from(self.positions.len()).expect("a map's vertices fit u32");
        for (corner, (uv, alpha)) in corners
            .into_iter()
            .zip(layer.uv.into_iter().zip(layer.alpha))
        {
            self.positions.push(placement.position(cells, corner));
            self.uvs.push([uv[0] / size[0], uv[1] / size[1]]);
            let [red, green, blue] = placement.light(placement.normal(cells, corner));
            self.colors
                .push([red, green, blue, f32::from(alpha) / 255.0]);
        }
        // Counter-clockwise seen from above, on the diagonal from corner 0 to 2, or 1 to 3.
        let triangles = if layer.flip {
            [1, 2, 3, 1, 3, 0]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        self.indices.extend(triangles.map(|corner| first + corner));
    }
}

impl Placement<'_> {
    /// Where the game's sample `[x, y]` lies, on the engine's axes: the engine's rows run from the
    /// game's north.
    #[expect(
        clippy::cast_precision_loss,
        reason = "texels, cells and steps of height lie far below 2²⁴, exact in an f32"
    )]
    fn position(self, cells: &TerrainCells<'_>, [x, y]: [usize; 2]) -> [f32; 3] {
        [
            self.origin[0] + x as f32 * self.cell,
            cells.height([x, y]) as f32 * self.step,
            self.origin[1] + (self.rows - 1 - y) as f32 * self.cell,
        ]
    }

    /// The game's normal at the sample `[x, y]`: the cross of its slopes east and north, each
    /// across its two neighbors, on the engine's axes.
    #[expect(
        clippy::cast_precision_loss,
        reason = "texels, cells and steps of height lie far below 2²⁴, exact in an f32"
    )]
    fn normal(self, cells: &TerrainCells<'_>, [x, y]: [usize; 2]) -> [f32; 3] {
        let [columns, rows] = cells.drawn();
        let height = |at| cells.height(at) as f32 * self.step;
        let east = height([(x + 1).min(columns), y]) - height([x.saturating_sub(1), y]);
        let north = height([x, (y + 1).min(rows)]) - height([x, y.saturating_sub(1)]);
        // (ACROSS, 0, east) × (0, ACROSS, north), its north turned to the engine's −z.
        let normal = [-ACROSS * east, ACROSS * ACROSS, ACROSS * north];
        let length = normal.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
        normal.map(|axis| axis / length)
    }

    /// The light the game gives a vertex of `normal`, linear: its ambient color, and each light's
    /// diffuse color by how far its ray, against the way it goes, meets the normal, from 0 to 1,
    /// each channel clamped to 0 to 1. The game multiplies a texture's sRGB codes by it, so it is
    /// an sRGB code itself, which linear light multiplies as the codes do.
    fn light(self, normal: [f32; 3]) -> [f32; 3] {
        let mut light = self.lighting.ambient;
        for each in &self.lighting.lights {
            let ray = each.direction.map(|axis| -axis);
            let meets = (0..3).map(|axis| ray[axis] * normal[axis]).sum::<f32>();
            let meets = meets.clamp(0.0, 1.0);
            for (channel, diffuse) in light.iter_mut().zip(each.diffuse) {
                *channel += meets * diffuse;
            }
        }
        let [red, green, blue] = light.map(|channel| channel.clamp(0.0, 1.0));
        let linear = LinearRgba::from(Srgba::new(red, green, blue, 1.0));
        [linear.red, linear.green, linear.blue]
    }
}

#[cfg(test)]
mod tests;
