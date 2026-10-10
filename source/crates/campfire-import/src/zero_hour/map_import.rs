use std::collections::BTreeSet;

use campfire_capabilities::HeightGrid;
use campfire_common::{Binary, Toml};
use campfire_math::Num;
use campfire_package::{MapLight, MapLights, Terrain, TerrainLight, TerrainLighting, TerrainParts};
use serde::Serialize;

use crate::zero_hour::error::MapError;
use crate::zero_hour::map_file::{GameLight, MapFile, MapHeights};
use crate::zero_hour::map_object::Placement;
use crate::zero_hour::unit_types::UnitTypes;

/// One map's files in the package: `map/<name>/map.toml`, `map/<name>/heights.bin`,
/// `client/maps/<name>/terrain.bin` and `client/maps/<name>/lights.toml`.
#[derive(Debug)]
pub(crate) struct MapImport {
    pub(crate) map: String,
    pub(crate) heights: Vec<u8>,
    pub(crate) terrain: Vec<u8>,
    pub(crate) lights: String,
}

/// A cell of the heightmap, 10 units wide (`MAP_XY_FACTOR`).
const CELL: i64 = 10;

/// A step of height, 0.625 units (`MAP_HEIGHT_SCALE`): 5 · 2⁻³, exact in a `Num`.
const STEP: Num = Num::from_bits(5 << (Num::FRAC_BITS - 3));

/// The team every placed unit stands on until the import reads the map's sides.
const NEUTRAL: &str = "neutral";

/// The tag of a marker made from a waypoint.
const WAYPOINT: &str = "waypoint";

#[derive(Debug, Serialize)]
struct MapToml {
    bounds: BoundsToml,
    units: Vec<UnitToml>,
    markers: Vec<MarkerToml>,
}

#[derive(Debug, Serialize)]
struct BoundsToml {
    min: [String; 2],
    max: [String; 2],
}

#[derive(Debug, Serialize)]
struct UnitToml {
    unit_type: String,
    team: &'static str,
    pos: [String; 2],
    angle: String,
    height: String,
}

#[derive(Debug, Serialize)]
struct MarkerToml {
    name: String,
    tags: [&'static str; 1],
    pos: [String; 2],
    params: MarkerParams,
}

#[derive(Debug, Serialize)]
struct MarkerParams {
    name: String,
}

impl MapImport {
    /// The files of `file`: its heights on the engine's axes, its terrain in the same order, lit
    /// by its first `lights` terrain lights, its objects as units of the types `unit_types`
    /// names, its waypoints as markers, bounds that hold its heights and every point it places,
    /// and its first `lights` object lights, as the game draws `NumberGlobalLights` of each. The
    /// terrain's ambient color is its first light's, and the objects' theirs, as the game takes
    /// them.
    pub(crate) fn new(
        file: &MapFile,
        unit_types: &mut UnitTypes,
        lights: usize,
    ) -> Result<MapImport, MapError> {
        let heights = MapImport::heights(&file.heights)?;
        let lighting = &file.lighting;
        let terrain_lighting = TerrainLighting {
            ambient: lighting.terrain[0].ambient,
            lights: lighting.terrain[..lights]
                .iter()
                .map(|light| TerrainLight {
                    diffuse: light.diffuse,
                    direction: MapImport::axes(light),
                })
                .collect(),
        };
        let map_lights = MapLights::new(
            lighting.objects[0].ambient,
            lighting.objects[..lights]
                .iter()
                .map(|light| MapLight {
                    color: light.diffuse,
                    direction: MapImport::axes(light),
                })
                .collect(),
        )
        .ok_or(MapError::Lighting)?;
        let terrain = Terrain::new(
            TerrainParts {
                cells: MapImport::flipped(&file.terrain.cells, file.heights.width),
                ..file.terrain.clone()
            },
            terrain_lighting,
        )
        .map_err(MapError::Terrain)?;
        let mut low = heights.origin();
        let mut high = heights.far_corner();
        let mut take = |point: [Num; 2]| {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
            point.map(|value| value.to_string())
        };
        let mut units = Vec::new();
        let mut markers = Vec::new();
        let mut waypoints = BTreeSet::new();
        for (at, object) in file.objects.iter().enumerate() {
            if object.road_or_bridge() {
                continue;
            }
            if let Some(waypoint) = &object.waypoint {
                let pos = take(object.ground().ok_or(MapError::Placement(at))?);
                let id = u32::try_from(waypoint.id)
                    .ok()
                    .ok_or(MapError::WaypointId(waypoint.id))?;
                if !waypoints.insert(id) {
                    return Err(MapError::WaypointTwice(waypoint.id));
                }
                let name = String::from_utf8(waypoint.name.clone())
                    .ok()
                    .filter(|name| name.is_ascii())
                    .ok_or(MapError::NotAscii)?;
                // A marker's param that reads as a decimal is taken for a number.
                if name.parse::<Num>().is_ok() {
                    return Err(MapError::WaypointName(name));
                }
                markers.push(MarkerToml {
                    name: format!("w{id}"),
                    tags: [WAYPOINT],
                    pos,
                    params: MarkerParams { name },
                });
            } else {
                let Placement {
                    ground,
                    height,
                    degrees,
                } = object.placement().ok_or(MapError::Placement(at))?;
                units.push(UnitToml {
                    unit_type: unit_types.name(&object.template)?.as_str().to_owned(),
                    team: NEUTRAL,
                    pos: take(ground),
                    angle: degrees.to_string(),
                    height: height.to_string(),
                });
            }
        }
        let map = MapToml {
            bounds: BoundsToml {
                min: low.map(|value| value.to_string()),
                max: high.map(|value| value.to_string()),
            },
            units,
            markers,
        };
        Ok(MapImport {
            map: Toml::write(&map).expect("a map encodes as TOML"),
            heights: Binary::encode(&heights),
            terrain: Binary::encode(&terrain),
            lights: Toml::write(&map_lights).expect("lights encode as TOML"),
        })
    }

    /// The way `light` goes on the engine's axes: the game's `x`, `y` and `z` are its `x`, `−z`
    /// and `y`.
    fn axes(light: &GameLight) -> [f32; 3] {
        let [x, y, z] = light.position;
        [x, z, -y]
    }

    /// The heights on the engine's axes. The original's sample of column `i` and row `j` lies at
    /// `x = (i − border) · 10` and its `y = (j − border) · 10`, which is the engine's `−z`: so
    /// the engine's rows run the file's in reverse, from `z = −(depth − 1 − border) · 10`.
    fn heights(heights: &MapHeights) -> Result<HeightGrid, MapError> {
        let MapHeights {
            width,
            depth,
            border,
            samples,
        } = heights;
        if border >= width || border >= depth {
            return Err(MapError::Heights);
        }
        let back = |count: usize| {
            i64::try_from(count)
                .ok()
                .and_then(|count| count.checked_mul(-CELL))
                .and_then(Num::from_int)
                .ok_or(MapError::Heights)
        };
        let origin = [back(*border)?, back(depth - 1 - border)?];
        HeightGrid::new(
            origin,
            Num::int(CELL),
            STEP,
            u32::try_from(*width).ok().ok_or(MapError::Heights)?,
            MapImport::flipped(samples, *width),
        )
        .ok_or(MapError::Heights)
    }

    /// `rows`, each `width` entries, in reverse.
    fn flipped<T: Copy>(rows: &[T], width: usize) -> Vec<T> {
        rows.rchunks_exact(width).flatten().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use campfire_capabilities::{
        Bounds, DeclaredName, MapData, MapPoint, MarkerData, ModeParam, PlacedUnitData, Scalar,
    };
    use campfire_package::{BlendShape, BlendTile};

    use super::*;
    use crate::zero_hour::map_file::internals::map;

    #[test]
    fn a_map_s_lights_are_its_time_of_day_s_on_the_engine_s_axes() {
        let file = MapFile::read(&map(8)).unwrap();
        let mut types = UnitTypes::default();
        let imported = MapImport::new(&file, &mut types, 3).unwrap();
        let terrain = Binary::decode::<Terrain>(&imported.terrain).unwrap();
        // The afternoon's lights: light `l` of time 2 has ambient [2, l, 0], diffuse [2, l, 1]
        // and position [2, l, 2], which is the engine's [2, 2, −l]. The terrain's ambient is its
        // light 0's, the objects' their light 3's.
        let terrain_light = |at: f32| TerrainLight {
            diffuse: [2.0, at, 1.0],
            direction: [2.0, 2.0, -at],
        };
        assert_eq!(
            *terrain.lighting(),
            TerrainLighting {
                ambient: [2.0, 0.0, 0.0],
                lights: vec![terrain_light(0.0), terrain_light(1.0), terrain_light(2.0)],
            }
        );
        let object_light = |at: f32| MapLight {
            color: [2.0, at, 1.0],
            direction: [2.0, 2.0, -at],
        };
        assert_eq!(
            Toml::parse::<MapLights>(&imported.lights).unwrap(),
            MapLights {
                ambient: [2.0, 3.0, 0.0],
                lights: vec![object_light(3.0), object_light(4.0), object_light(5.0)],
            }
        );
        // Drawn in two lights, each keeps its first two.
        let two = MapImport::new(&file, &mut types, 2).unwrap();
        let terrain = Binary::decode::<Terrain>(&two.terrain).unwrap();
        assert_eq!(terrain.lighting().lights.len(), 2);
        assert_eq!(
            Toml::parse::<MapLights>(&two.lights).unwrap().lights.len(),
            2
        );
        // An object light it draws that goes no way is refused; one it does not draw is not.
        let mut still = file;
        still.lighting.objects[1].position = [0.0; 3];
        let refused = MapImport::new(&still, &mut types, 3).map(drop);
        assert_eq!(refused, Err(MapError::Lighting));
        assert!(MapImport::new(&still, &mut types, 1).is_ok());
    }

    #[test]
    fn a_map_imports_to_its_heights_units_and_markers_computed_by_hand() {
        let file = MapFile::read(&map(8)).unwrap();
        let mut types = UnitTypes::default();
        let imported = MapImport::new(&file, &mut types, 3).unwrap();

        // 4 × 3 samples, a border of 1: x from −10 to 20, and z from −(3 − 1 − 1) · 10 = −10 to
        // 10, the file's last row first.
        let heights = Binary::decode::<HeightGrid>(&imported.heights).unwrap();
        let expected = HeightGrid::new(
            [Num::int(-10), Num::int(-10)],
            Num::int(10),
            Num::int(5) / Num::int(8),
            4,
            vec![8, 20, 10, 40, 16, 5, 6, 7, 0, 1, 2, 3],
        )
        .unwrap();
        assert_eq!(heights, expected);

        // The file's row 2 is the engine's row 0, so its cell of column x and row y is the
        // engine's (2 − y) · 4 + x: its last, a cliff of tile 3, the engine's 3; its first, a
        // cliff of tile 0, the engine's 8; its cell 2, of cliff UVs, the engine's 10.
        let terrain = Binary::decode::<Terrain>(&imported.terrain).unwrap();
        let cells = &terrain.parts().cells;
        assert_eq!(cells.len(), 12);
        assert!(cells[3].cliff && cells[3].tile == 3);
        assert!(cells[8].cliff && cells[8].tile == 0);
        assert_eq!(cells[10].cliff_uv, Some(0));
        assert_eq!(cells[5].blend, Some(0));
        assert_eq!(cells[6].extra_blend, Some(0));
        assert_eq!(cells.iter().filter(|cell| cell.cliff).count(), 2);
        assert_eq!(
            terrain.parts().blends,
            [BlendTile {
                tile: 4,
                shape: BlendShape::LeftDiagonal { long: true },
                inverted: true,
                flipped: true,
                custom_edge: None,
            }]
        );

        // The tank at the original's (30, 5) stands at x 30, z −5, turned π/2 in `f32`,
        // 1,509,949,482 steps of 2⁻²⁴ degrees. The rock at (−10, 15), 4.5 below the ground,
        // turned 4.729842 − 2π = −1.5533433 in `f32`, −1,493,172,475.64 steps, so
        // −1,493,172,476. The waypoint at (12.5, 7.5) is marker `w3`. The road's end is none.
        let map: MapData = Toml::parse(&imported.map).unwrap();
        let decimal = |text: &str| Scalar::Decimal(text.parse().unwrap());
        let ground = |x: &str, z: &str| MapPoint::Ground([decimal(x), decimal(z)]);
        let unit = |unit_type: &str, pos, angle: i64, height: Num| PlacedUnitData {
            unit_type: DeclaredName::new(unit_type).unwrap(),
            team: DeclaredName::new("neutral").unwrap(),
            pos,
            angle: Num::from_bits(angle),
            height,
            path: None,
            from: None,
        };
        assert_eq!(
            map.units,
            [
                unit(
                    "americatankcrusader",
                    ground("30", "-5"),
                    1_509_949_482,
                    Num::ZERO
                ),
                unit(
                    "rocks1",
                    ground("-10", "-15"),
                    -1_493_172_476,
                    Num::int(-9) / Num::int(2)
                ),
            ]
        );
        assert_eq!(
            map.markers,
            [MarkerData {
                name: DeclaredName::new("w3").unwrap(),
                tags: vec![DeclaredName::new("waypoint").unwrap()],
                pos: Some(ground("12.5", "-7.5")),
                region: None,
                team: None,
                params: [(
                    DeclaredName::new("name").unwrap(),
                    ModeParam::Text("Player_1_Start".to_owned()),
                )]
                .into(),
                events: false,
            }]
        );
        // The bounds hold the heights and reach the tank's x of 30 and the rock's z of −15.
        let bounds: Bounds = Toml::parse("min = [-10, -15]\nmax = [30, 10]").unwrap();
        assert_eq!(map.bounds, bounds);
        assert_eq!(
            types.toml(),
            "[units.americatankcrusader]\n\n[units.rocks1]\n"
        );
    }

    #[test]
    fn a_map_whose_waypoints_or_objects_the_engine_cannot_hold_is_refused() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let import = |file: &MapFile| MapImport::new(file, &mut UnitTypes::default(), 3).map(drop);
        let waypoint = 2;
        let mut twice = file.clone();
        twice.objects.push(file.objects[waypoint].clone());
        assert_eq!(import(&twice), Err(MapError::WaypointTwice(3)));
        let mut numbered = file.clone();
        numbered.objects[waypoint].waypoint.as_mut().unwrap().name = b"12.5".to_vec();
        assert_eq!(
            import(&numbered),
            Err(MapError::WaypointName("12.5".to_owned()))
        );
        let mut negative = file.clone();
        negative.objects[waypoint].waypoint.as_mut().unwrap().id = -1;
        assert_eq!(import(&negative), Err(MapError::WaypointId(-1)));
        // A waypoint reads no angle and no height, so neither an infinite angle nor an infinite
        // height refuses it; its ground point past the world's bound does.
        let mut turned = file.clone();
        turned.objects[waypoint].angle = f32::INFINITY;
        turned.objects[waypoint].pos[2] = f32::INFINITY;
        assert_eq!(import(&turned), Ok(()));
        turned.objects[waypoint].pos[1] = f32::INFINITY;
        assert_eq!(import(&turned), Err(MapError::Placement(waypoint)));
        file.objects[1].pos[0] = f32::INFINITY;
        assert_eq!(import(&file), Err(MapError::Placement(1)));
        file.objects[1].pos[0] = 0.0;
        file.objects[1].angle = f32::INFINITY;
        assert_eq!(import(&file), Err(MapError::Placement(1)));
    }
}
