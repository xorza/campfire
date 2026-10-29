use campfire_client_files::FogLevel;
use campfire_geometry::{Bounds, Grid};
use campfire_math::Num;
use campfire_vision::BrushMap;

use super::*;

#[test]
fn a_cell_keeps_the_light_of_seen_explored_or_shrouded_and_a_known_map_shrouds_none() {
    // Zero Hour's fog over 2 × 2 cells: cell 0 seen, cells 0 and 1 explored.
    let file = FogFile::new([255, 127, 0].map(FogLevel), [255; 3]).unwrap();
    let mut shrouded = MatchFog {
        file,
        columns: 2,
        rows: 2,
        frame: [0.0, 0.0, 0.5, 0.5],
        shroud: true,
    };
    let seen = CellBits::of_cells(4, &[0]);
    let explored = CellBits::of_cells(4, &[0, 1]);
    let mut texels = [0; 16];
    let cells = |seen, explored| FogSight::Cells { seen, explored };
    shrouded.texels(cells(&seen, &explored), &mut texels);
    let (clear, fogged, dark) = ([255; 4], [127, 127, 127, 255], [0, 0, 0, 255]);
    assert_eq!(texels, [clear, fogged, dark, dark].concat()[..]);
    // A map that starts known fogs each cell its team does not see.
    shrouded.shroud = false;
    shrouded.texels(cells(&seen, &CellBits::default()), &mut texels);
    assert_eq!(texels, [clear, fogged, fogged, fogged].concat()[..]);
    // A spectator's fog keeps every cell's light.
    shrouded.texels(FogSight::All, &mut texels);
    assert_eq!(texels, [clear; 4].concat()[..]);
    // Its cells of 1 m from the origin: a point's cell, row by row, none past the grid.
    let at = |x: f32, z: f32| shrouded.cell_at(Vec3::new(x, 7.0, z));
    assert_eq!(
        [at(0.5, 0.5), at(1.5, 0.5), at(0.5, 1.5), at(1.99, 1.99)],
        [Some(0), Some(1), Some(2), Some(3)]
    );
    assert_eq!([at(-0.1, 0.5), at(2.0, 0.5), at(0.5, 2.0)], [None; 3]);
}

#[test]
fn the_frame_places_the_texture_on_the_grid_s_whole_cells() {
    // Tournament Desert's bounds, x −350 to 2340 by z −3040 to 350, in cells of 40 m: 68 columns,
    // 2720 m, and 85 rows, 3400 m, past the bounds' 2690 and 3390.
    let bounds = Bounds::new(
        [Num::int(-350), Num::int(-3040)],
        [Num::int(2340), Num::int(350)],
    );
    let grid = Grid::new(Num::int(40), bounds.unwrap()).unwrap();
    let vision = VisionGrid {
        brush: BrushMap::new(&grid, &[]),
        grid,
        teams: 2,
        shroud: true,
        neutral_in_fog: false,
    };
    assert_eq!(
        MatchFog::frame(&vision),
        [-350.0, -3040.0, 1.0 / 2720.0, 1.0 / 3400.0]
    );
}
