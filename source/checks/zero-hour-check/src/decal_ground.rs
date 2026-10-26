use campfire_client_files::TerrainFile;
use campfire_math::Num;
use campfire_units::HeightGrid;

/// How a terrain's decals lie on its ground (roads.md, A decal lies on the ground): how many of
/// its decal vertices stand off the ground, as the client draws it, plus the float by more than
/// `TOLERANCE`, and the most any stands off it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DecalGround {
    pub(crate) off: usize,
    pub(crate) most: f64,
}

/// How far a vertex may stand off: 16 of `f32`'s ulps at heights from 128 to 256, 2⁻¹⁶ each,
/// for the rounding of the corners the import makes where a piece crosses the cells' sides and
/// diagonals and its texture's repeats, where a hang stands a share of a step, 0.625, off.
const TOLERANCE: f64 = 1.0 / 4096.0;

impl DecalGround {
    /// The decals of `terrain` over `grid`, a road's float an eighth of the grid's step, as
    /// Zero Hour's `FLOAT_AMOUNT`.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a Num's bits and a grid's columns and rows lie far below 2⁵³"
    )]
    pub(crate) fn of(grid: &HeightGrid, terrain: &TerrainFile) -> Self {
        let float = |num: Num| num.to_bits() as f64 / f64::from(1_u32 << Num::FRAC_BITS);
        let [x0, z0] = grid.origin().map(float);
        let [cell, step] = [grid.cell(), grid.step()].map(float);
        let columns = grid.columns() as usize;
        let rows = grid.rows() as usize;
        let samples = grid.samples();
        let sample = |column: usize, row: usize| f64::from(samples[row * columns + column]);
        let quads = &terrain.quads().ground;
        // The client's ground: a point's cell is its place over the grid floored, its triangle
        // the one its quad's split holds it in, and past the grid the edge's nearest point.
        let ground = |x: f64, z: f64| {
            let u = ((x - x0) / cell).clamp(0.0, (columns - 1) as f64);
            let v = ((z - z0) / cell).clamp(0.0, (rows - 1) as f64);
            let [column, row] = [(u, columns), (v, rows)].map(|(at, count)| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a clamped place over the grid, of no sign"
                )]
                let floored = at.floor() as usize;
                floored.min(count - 2)
            });
            let [fu, fv] = [u - column as f64, v - row as f64];
            let [h0, h1, h2, h3] = [
                sample(column, row),
                sample(column + 1, row),
                sample(column + 1, row + 1),
                sample(column, row + 1),
            ];
            let height = if quads[row * (columns - 1) + column].flip {
                if fu + fv <= 1.0 {
                    h0 + fu * (h1 - h0) + fv * (h3 - h0)
                } else {
                    h2 + (1.0 - fu) * (h3 - h2) + (1.0 - fv) * (h1 - h2)
                }
            } else if fu >= fv {
                h0 + fu * (h1 - h0) + fv * (h2 - h1)
            } else {
                h0 + fv * (h3 - h0) + fu * (h2 - h3)
            };
            height * step + step / 8.0
        };
        let mut decals = Self { off: 0, most: 0.0 };
        for vertex in &terrain.decals().vertices {
            let at = vertex.position.as_dvec3();
            let off = (at.y - ground(at.x, at.z)).abs();
            decals.most = decals.most.max(off);
            if off > TOLERANCE {
                decals.off += 1;
            }
        }
        decals
    }
}
