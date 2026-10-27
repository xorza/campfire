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
            shroud: false,
            neutral_in_fog: false,
        })
    );
    // A grid of no brush that starts known writes its cell alone; a shrouded one says so.
    let plain = GridData {
        cell: Num::int(2),
        brush: Vec::new(),
        shroud: false,
        neutral_in_fog: false,
    };
    assert_eq!(Toml::write(&plain), "cell = \"2\"\n");
    let shrouded = GridData {
        shroud: true,
        ..plain.clone()
    };
    assert_eq!(Toml::write(&shrouded), "cell = \"2\"\nshroud = true\n");
    let neutral = GridData {
        neutral_in_fog: true,
        ..plain
    };
    assert_eq!(
        Toml::parse::<GridData>("cell = 2\nneutral_in_fog = true"),
        Ok(neutral)
    );
    assert_eq!(
        Toml::parse::<GridData>("cell = 2\nshroud = true"),
        Ok(shrouded)
    );
}
