use bevy::asset::RenderAssetUsages;
use bevy::math::{Vec2, Vec3, Vec4};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use campfire_client_files::{TerrainDecals, TerrainFile};
use campfire_units::HeightGrid;

use crate::view::float_num::FloatNum;

/// A terrain file's two meshes over its map's height grid (02-engine-core.md, A terrain is quads
/// over the height grid; 02-engine-core.md, The terrain is lit as every model is): the ground, each
/// cell's quad; and the overlays, in the file's order, then its decals, in theirs, so one draw
/// blends them in that order. Each quad has its own four vertices, at the grid's samples of its
/// cell's corners, with the sample's normal and the quad's texture coordinates as shares of the
/// texture; an overlay's alpha at each corner is its vertices'. A decal's vertex keeps its point
/// and takes the normal of the sample nearest it, as the ground under it is lit, and an alpha of
/// 1, so its texture's alpha alone blends it.
#[derive(Debug)]
pub(crate) struct TerrainMeshes {
    pub(crate) ground: TerrainMesh,
    pub(crate) overlay: TerrainMesh,
}

/// A mesh of quads; its colors are an overlay's alone.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TerrainMesh {
    pub(crate) positions: Vec<Vec3>,
    pub(crate) normals: Vec<Vec3>,
    pub(crate) uvs: Vec<Vec2>,
    pub(crate) colors: Vec<Vec4>,
    pub(crate) indices: Vec<u32>,
}

/// A quad of the grid: its cell, its texture coordinates in texels at each corner, and the diagonal
/// its triangles meet on.
#[derive(Debug, Clone, Copy)]
struct CellQuad {
    cell: usize,
    uv: [Vec2; 4],
    diagonal: Diagonal,
}

/// The diagonal a quad's two triangles meet on, by its corners, which go round the cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Diagonal {
    ZeroTwo,
    OneThree,
}

/// The grid's samples on the engine's axes, each with its normal, row after row, `cell` apart.
#[derive(Debug)]
struct Samples {
    columns: usize,
    cell: f32,
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
}

impl TerrainMeshes {
    /// The meshes of `file` over `grid`, whose cells its quads are.
    pub(crate) fn of(file: &TerrainFile, grid: &HeightGrid) -> Self {
        let quads = file.quads();
        debug_assert!(
            quads.fits(grid.columns(), grid.rows()),
            "a terrain over the grid's cells"
        );
        let samples = Samples::of(grid);
        // A file's texture sides lie far below 2²⁴, exact in an f32.
        let size = file.texture().size.as_vec2();
        let mut ground = TerrainMesh::default();
        for (cell, quad) in quads.ground.iter().enumerate() {
            let quad = CellQuad {
                cell,
                uv: quad.uv,
                diagonal: Diagonal::of(quad.flip),
            };
            ground.quad(&samples, quad, size);
        }
        let mut overlay = TerrainMesh::default();
        for each in &quads.overlays {
            let quad = CellQuad {
                cell: each.cell as usize,
                uv: each.uv,
                diagonal: Diagonal::of(each.flip),
            };
            overlay.quad(&samples, quad, size);
            overlay.colors.extend(
                each.alpha
                    .map(|alpha| Vec4::new(1.0, 1.0, 1.0, f32::from(alpha) / 255.0)),
            );
        }
        overlay.decals(&samples, file.decals(), size);
        Self { ground, overlay }
    }
}

impl TerrainMesh {
    /// The mesh for Bevy, its vertices and indices in the render world alone.
    pub(crate) fn into_mesh(self) -> Mesh {
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices));
        if self.colors.is_empty() {
            mesh
        } else {
            mesh.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        }
    }

    /// Adds `quad`, of a texture `size` texels.
    fn quad(&mut self, samples: &Samples, quad: CellQuad, size: Vec2) {
        let CellQuad { cell, uv, diagonal } = quad;
        let first = u32::try_from(self.positions.len()).expect("a map's vertices fit u32");
        let cells = samples.columns - 1;
        let at = cell / cells * samples.columns + cell % cells;
        let corners = [at, at + 1, at + 1 + samples.columns, at + samples.columns];
        for (corner, uv) in corners.into_iter().zip(uv) {
            self.positions.push(samples.positions[corner]);
            self.normals.push(samples.normals[corner]);
            self.uvs.push(uv / size);
        }
        // The grid's rows run to the engine's +z, so its corners go clockwise seen from above,
        // and each triangle takes them backwards to face up.
        let triangles = match diagonal {
            Diagonal::OneThree => [0, 3, 1, 1, 3, 2],
            Diagonal::ZeroTwo => [0, 2, 1, 0, 3, 2],
        };
        self.indices.extend(triangles.map(|corner| first + corner));
    }
}

impl TerrainMesh {
    /// Adds `decals`, of a texture `size` texels, each vertex with the normal of the sample of
    /// `samples` nearest it, a tie the sample of the greater coordinate.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a sample's number is whole and at least 0, and the cast saturates past the grid"
    )]
    fn decals(&mut self, samples: &Samples, decals: &TerrainDecals, size: Vec2) {
        let first = u32::try_from(self.positions.len()).expect("a map's vertices fit u32");
        let rows = samples.positions.len() / samples.columns;
        let origin = samples.positions[0];
        let cell = samples.cell;
        let nearest = |at: f32, from: f32, count: usize| {
            let at = ((at - from) / cell + 0.5).floor().max(0.0);
            (at as usize).min(count - 1)
        };
        for vertex in &decals.vertices {
            let [x, _, z] = vertex.position.to_array();
            let column = nearest(x, origin.x, samples.columns);
            let row = nearest(z, origin.z, rows);
            self.positions.push(vertex.position);
            self.normals
                .push(samples.normals[row * samples.columns + column]);
            self.uvs.push(vertex.uv / size);
            self.colors.push(Vec4::ONE);
        }
        self.indices.extend(
            decals
                .triangles
                .iter()
                .flat_map(|triangle| triangle.map(|at| first + at)),
        );
    }
}

impl Diagonal {
    /// The diagonal of a quad a terrain file gives `flipped` or not: from corner 1 to 3 when
    /// flipped, else from 0 to 2.
    const fn of(flipped: bool) -> Self {
        if flipped {
            Self::OneThree
        } else {
            Self::ZeroTwo
        }
    }
}

impl Samples {
    /// The samples of `grid`: each at `origin + [column, row] · cell` at its height, its normal
    /// across its two neighbors on each axis, the slope over the distance between them, so one
    /// the map's edge clamps to the sample itself spans one cell, not two.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a grid's columns, rows and heights lie far below 2²⁴, exact in an f32"
    )]
    fn of(grid: &HeightGrid) -> Self {
        let columns = grid.columns() as usize;
        let rows = grid.rows() as usize;
        let [x, z] = grid.origin().map(FloatNum::float);
        let [cell, step] = [grid.cell().float(), grid.step().float()];
        let height =
            |column: usize, row: usize| f32::from(grid.samples()[row * columns + column]) * step;
        let mut positions = Vec::with_capacity(grid.samples().len());
        let mut normals = Vec::with_capacity(grid.samples().len());
        for row in 0..rows {
            for column in 0..columns {
                positions.push(Vec3::new(
                    x + column as f32 * cell,
                    height(column, row),
                    z + row as f32 * cell,
                ));
                let [west, east] = [column.saturating_sub(1), (column + 1).min(columns - 1)];
                let [north, south] = [row.saturating_sub(1), (row + 1).min(rows - 1)];
                let slope_x =
                    (height(east, row) - height(west, row)) / ((east - west) as f32 * cell);
                let slope_z = (height(column, south) - height(column, north))
                    / ((south - north) as f32 * cell);
                normals.push(Vec3::new(-slope_x, 1.0, -slope_z).normalize());
            }
        }
        Self {
            columns,
            cell,
            positions,
            normals,
        }
    }
}

#[cfg(test)]
mod tests;
