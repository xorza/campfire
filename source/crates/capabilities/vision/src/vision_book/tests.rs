use campfire_geometry::{Bounds, PolygonError};
use campfire_units::{MapPoint, MapPointError, MapSpace};

use super::*;
use crate::grid_data::BrushData;

#[test]
fn a_brush_loads_only_as_a_simple_polygon_within_the_bounds() {
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
    let map = MapBook {
        space: MapSpace::default(),
        bounds,
    };
    let with = |points: &[(i64, i64)]| {
        let sections = Sections::default().with(GridData {
            cell: Num::ONE,
            brush: vec![BrushData {
                points: points
                    .iter()
                    .map(|&(x, z)| MapPoint::ground(x, z))
                    .collect(),
            }],
        });
        VisionBook::of(&map, &sections, 2)
    };
    let fault = |problem| Err(VisionDataError::Brush { brush: 0, problem });
    assert_eq!(
        with(&[(0, 0), (4, 0), (0, 5)]),
        Err(VisionDataError::Point(MapPointError::OutOfBounds))
    );
    assert_eq!(with(&[(0, 0), (4, 0)]), fault(PolygonError::TooFewPoints));
    let crossed = PolygonError::EdgesMeet {
        first: 0,
        second: 2,
    };
    assert_eq!(with(&[(0, 0), (4, 4), (4, 0), (0, 4)]), fault(crossed));
    let triangle = with(&[(0, 0), (4, 0), (0, 4)]).unwrap();
    let grid = triangle.grid().unwrap();
    assert_eq!((grid.teams, grid.grid.cells()), (2, 16));
    // A map with no `[grid]` has no vision grid.
    assert_eq!(
        VisionBook::of(&map, &Sections::default(), 2),
        Ok(VisionBook(None))
    );
    // A grid holds 64 teams, and refuses a 65th.
    let grid = Sections::default().with(GridData {
        cell: Num::ONE,
        brush: Vec::new(),
    });
    assert!(VisionBook::of(&map, &grid, 64).is_ok());
    assert_eq!(
        VisionBook::of(&map, &grid, 65),
        Err(VisionDataError::TooManyTeams)
    );
    // 2²³ cells times teams are the most: a grid of 2²¹ cells, 2,048 by 1,024 of them, holds
    // four teams, and refuses a fifth.
    let wide = MapBook {
        space: MapSpace::default(),
        bounds: Bounds::new([Num::ZERO; 2], [Num::int(2_048), Num::int(1_024)]).unwrap(),
    };
    assert!(VisionBook::of(&wide, &grid, 4).is_ok());
    assert_eq!(
        VisionBook::of(&wide, &grid, 5),
        Err(VisionDataError::TooLarge)
    );
}
