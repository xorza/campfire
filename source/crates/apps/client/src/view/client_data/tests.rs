use bevy::math::{UVec2, Vec2};
use campfire_client_files::{GroundQuad, TerrainQuads, TerrainSurface, TerrainTexture};
use campfire_math::Num;
use campfire_store::internals::TestDir;

use super::*;
use crate::view::package_source::samples::package;

#[test]
fn a_package_gives_its_units_and_skips_a_file_that_does_not_read() {
    let test_dir = TestDir::new();
    let units = br#"[units.tank]
models = [{ model = "client/models/tank.glb", hide = ["tank.flash"] }]
"#;
    let files = package(&test_dir, "mode", &[("client/units.toml", units)]);
    let none = package(&test_dir, "bare", &[("data/units.toml", b"")]);
    let broken = package(&test_dir, "broken", &[("client/units.toml", b"units = 1")]);
    let mut data = ClientData::default();
    data.read_package(PackageIndex::MODE, &files);
    data.read_package(PackageIndex::new(1), &none);
    data.read_package(PackageIndex::new(2), &broken);
    // The tank's model, with the node it hides; a package with no units has none, as one whose
    // units do not read.
    let tank = &data.of(PackageIndex::MODE).units.units["tank"].models[0];
    assert_eq!(
        (tank.model.as_str(), tank.hide.as_slice()),
        ("client/models/tank.glb", &["tank.flash".to_owned()][..])
    );
    assert!(data.of(PackageIndex::new(1)).units.units.is_empty());
    assert!(data.of(PackageIndex::new(2)).units.units.is_empty());
}

#[test]
fn a_map_gives_its_lights_and_its_client_objects_and_skips_what_does_not_read() {
    let test_dir = TestDir::new();
    let lights = b"ambient = [0.25, 0.25, 0.25]
lights = [{ color = [1.0, 1.0, 1.0], direction = [0.0, -1.0, 0.0] }]
";
    let objects = br#"[[objects]]
look = "oak"
position = [3, "-2.5"]
height = 1
angle = 90
"#;
    let looks = Toml::parse::<ClientUnits>(
        r#"[units.lamp]
models = [{ model = "client/models/lamp.glb" }]
variants = [{ conditions = ["night"], models = [{ model = "client/models/lit.glb" }] }]
"#,
    )
    .unwrap();
    let map = MapName::parse("desert").unwrap();
    let mode = package(
        &test_dir,
        "mode",
        &[
            ("client/maps/desert/lights.toml", lights),
            ("client/maps/desert/objects.toml", objects),
            (
                "client/maps/desert/conditions.toml",
                b"conditions = [\"night\"]",
            ),
        ],
    );
    let mut data = ClientData {
        packages: vec![PackageClient {
            units: looks.clone(),
        }],
        ..ClientData::default()
    };
    data.read_map(&mode, &map, None);
    // The map's lights, and its one oak at (3, −2.5), 1 m up, turned 90°. A night map: the lamp
    // draws its night models, and keeps no variant.
    assert_eq!(data.lights.as_ref().unwrap().lights.len(), 1);
    let lamp = &data.of(PackageIndex::MODE).units.units["lamp"];
    assert_eq!(lamp.models[0].model.as_str(), "client/models/lit.glb");
    assert!(lamp.variants.is_empty());
    let oak = &data.objects.objects[0];
    assert_eq!(data.objects.objects.len(), 1);
    assert_eq!(oak.look.as_str(), "oak");
    assert_eq!(oak.position, [Num::int(3), Num::int(-5) / Num::int(2)]);
    assert_eq!((oak.height, oak.angle), (Num::ONE, Num::int(90)));
    // A map of no objects file has none, as one whose file does not read; no heights or
    // camera file gives none either. No conditions file: the lamp draws its own models.
    let broken = package(
        &test_dir,
        "broken",
        &[(
            "client/maps/desert/objects.toml",
            b"[[objects]]\nlook = 1\n",
        )],
    );
    for files in [package(&test_dir, "bare", &[]), broken] {
        let mut data = ClientData {
            packages: vec![PackageClient {
                units: looks.clone(),
            }],
            ..ClientData::default()
        };
        data.read_map(&files, &map, None);
        let lamp = &data.of(PackageIndex::MODE).units.units["lamp"];
        assert_eq!(lamp.models[0].model.as_str(), "client/models/lamp.glb");
        assert!(data.objects.objects.is_empty());
        assert!(data.lights.is_none() && data.camera.is_none() && data.heights.is_none());
    }
}

#[test]
fn a_map_keeps_its_terrain_over_its_heights_and_splits_their_cells_as_it_draws_them() {
    let test_dir = TestDir::new();
    let map = MapName::parse("desert").unwrap();
    // 3 × 2 samples, 10 m apart from (0, 0): 0, 16 and 0 steps of 0.625 m on row 0, then 16, 0
    // and 16, so the cells' two diagonals differ.
    let grid = HeightGrid::new(
        [Num::ZERO, Num::ZERO],
        Num::int(10),
        Num::from_bits(5 << (Num::FRAC_BITS - 3)),
        3,
        vec![0, 16, 0, 16, 0, 16],
    )
    .unwrap();
    let terrain = |columns: u32, flips: &[bool]| {
        let quads = TerrainQuads {
            columns,
            ground: flips
                .iter()
                .map(|&flip| GroundQuad {
                    uv: [Vec2::ZERO; 4],
                    flip,
                })
                .collect(),
            overlays: Vec::new(),
        };
        let texture = TerrainTexture {
            size: UVec2::splat(4),
            levels: 1,
            copies: Vec::new(),
            wraps: Vec::new(),
        };
        let surface = TerrainSurface {
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
        };
        Binary::encode(&TerrainFile::new(quads, texture, surface).unwrap())
    };
    let read = |name: &str, terrain: &[u8]| {
        let files = package(
            &test_dir,
            name,
            &[("client/maps/desert/terrain.bin", terrain)],
        );
        let mut data = ClientData::default();
        data.read_map(&files, &map, Some(grid.clone()));
        data
    };
    // A terrain of the grid's two cells, the first flipped, is kept, and the heights split as it
    // draws them: at (5, 5), the middle of cell 0, on its diagonal from corner 1 to 3, halfway
    // from sample [1, 0]'s 10 m to [0, 1]'s 10 m, so 10 m; split the other way, halfway from
    // [0, 0]'s 0 to [1, 1]'s 0, so 0.
    let fits = read("fits", &terrain(3, &[true, false]));
    assert!(fits.terrain.is_some());
    assert_eq!(fits.heights.as_ref().unwrap().at(5.0, 5.0), 10.0);
    let other = read("other", &terrain(3, &[false, false]));
    assert_eq!(other.heights.as_ref().unwrap().at(5.0, 5.0), 0.0);
    // A terrain of another shape, two columns, or two rows of cells, is dropped, logged, and the
    // heights split each cell from its first sample.
    for (name, columns, flips) in [
        ("wide", 2, &[true, true, true][..]),
        ("long", 3, &[true; 4][..]),
    ] {
        let data = read(name, &terrain(columns, flips));
        assert!(data.terrain.is_none(), "{name}");
        assert_eq!(data.heights.as_ref().unwrap().at(5.0, 5.0), 0.0, "{name}");
    }
}
