use campfire_codec::Toml;

use super::*;

#[test]
fn a_grid_reads_its_cell_and_its_brush() {
    let text = "cell = 2\nbrush = [{ points = [[0, 0, 0], [2, 0, 0], [0, 0, 2]] }]";
    let space = |x, y, z| MapPoint::Space([x, y, z].map(Num::int));
    assert_eq!(
        Toml::parse::<GridData>(text),
        Ok(GridData {
            cell: Num::int(2),
            brush: vec![BrushData {
                points: vec![space(0, 0, 0), space(2, 0, 0), space(0, 0, 2)],
            }],
        })
    );
    // A grid of no brush writes its cell alone.
    let plain = GridData {
        cell: Num::int(2),
        brush: Vec::new(),
    };
    assert_eq!(Toml::write(&plain), "cell = \"2\"\n");
}
