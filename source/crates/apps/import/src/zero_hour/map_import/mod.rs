use std::collections::{BTreeMap, BTreeSet};

use bevy_math::Vec3;
use campfire_client_files::{
    ClientFile, ClientObject, ClientObjects, MapConditions, MapLight, MapLights, ObjectLook,
    TerrainFile, TerrainFileFields, TerrainSurface,
};
use campfire_codec::{Binary, Toml};
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_geometry::Bounds;
use campfire_math::Num;
use campfire_mode::{MapData, MapPatch, MarkerData, ModeParam, PlacedUnitData};
use campfire_navigation::MapNavigationData;
use campfire_units::{BoundsData, CellSplit, CoarseSpan, Ground, GroundData, HeightGrid, MapPoint};
use campfire_values::Scalar;
use campfire_vision::GridData;

use crate::zero_hour::axes;
use crate::zero_hour::axes::cell_frame::CellFrame;
use crate::zero_hour::cliff_walls::CliffWalls;
use crate::zero_hour::dropped_roads::DroppedRoads;
use crate::zero_hour::error::MapError;
use crate::zero_hour::format::ini::game_data_ini::TerrainDraw;
use crate::zero_hour::format::ini::object_ini::ini_object::PlacedAs;
use crate::zero_hour::format::map::map_file::{MapHeights, ZeroHourMap};
use crate::zero_hour::format::map::map_object::ObjectPlace;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::import_team::{ImportTeam, SLOTS};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::map_roads::MapRoads;
use crate::zero_hour::road_buffer::RoadBuffer;
use crate::zero_hour::road_mesh::RoadGround;
use crate::zero_hour::road_pieces::RoadPieces;
use crate::zero_hour::terrain_atlas::{PackageTexture, TerrainAtlas};
use crate::zero_hour::terrain_cells::TerrainCells;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// One map's files in the package: `map/<name>/map.toml`, `map/<name>/heights.bin`,
/// `client/maps/<name>/terrain.bin`, `client/maps/<name>/lights.toml` and
/// `client/maps/<name>/objects.toml` and `client/maps/<name>/conditions.toml`; how many cells its
/// terrain draws a tile of with no texture; the road pieces it draws, and those it does not draw
/// as the game does; and the names of the waypoints it dropped, past its bounds.
#[derive(Debug)]
pub(crate) struct MapImport {
    pub(crate) map: String,
    pub(crate) heights: Vec<u8>,
    pub(crate) terrain: Vec<u8>,
    pub(crate) untextured: usize,
    pub(crate) road_pieces: RoadPieces,
    pub(crate) dropped_roads: Vec<DroppedRoads>,
    pub(crate) lights: Vec<u8>,
    pub(crate) objects: Vec<u8>,
    pub(crate) conditions: Vec<u8>,
    pub(crate) dropped_waypoints: Vec<String>,
}

/// A waypoint of a map's objects: its id, its name and its ground point.
#[derive(Debug)]
struct Waypoint {
    id: u32,
    name: String,
    position: [Num; 2],
}

/// A waypoint a skirmish starts a player slot's player at, by the player slot's number from 1: its
/// start, or its rally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkirmishPoint {
    Start(u8),
    Rally(u8),
}

impl Waypoint {
    /// The skirmish's point it is, as `placeNetworkBuildingsForPlayer` finds them by name:
    /// `Player_<n>_Start` or `Player_<n>_Rally`, `n` from 1 to `SLOTS`, of that case.
    fn skirmish(&self) -> Option<SkirmishPoint> {
        let rest = self.name.strip_prefix("Player_")?;
        let (slot, point): (&str, fn(u8) -> SkirmishPoint) = match rest.strip_suffix("_Start") {
            Some(slot) => (slot, SkirmishPoint::Start),
            None => (rest.strip_suffix("_Rally")?, SkirmishPoint::Rally),
        };
        (1..=SLOTS).find(|n| n.to_string() == slot).map(point)
    }
}

impl SkirmishPoint {
    /// The number of its player slot, from 1.
    const fn slot(self) -> u8 {
        let (Self::Start(n) | Self::Rally(n)) = self;
        n
    }
}

/// What a map import reads and adds to beside its file: the install's rules, of the objects and the
/// locomotors they name, the names the objects took so far, with the unit types of the maps, the
/// object looks of the package's `client/units.toml`, how many of the map's lights the game draws,
/// the settings of `GameData.ini` its terrain is drawn by, and its roads.
#[derive(Debug)]
pub(crate) struct MapSources<'a, 'r> {
    pub(crate) rules: &'r ZeroHourRules,
    pub(crate) names: &'a mut ImportNames<'r>,
    pub(crate) object_looks: &'a BTreeMap<DeclaredName, ObjectLook>,
    pub(crate) lights: usize,
    pub(crate) game: TerrainDraw,
    /// The texture of each of the map's texture classes, in their order.
    pub(crate) textures: &'a [Option<PackageTexture>],
    pub(crate) roads: &'a MapRoads,
    /// The texture of each of the roads' types, in their order, none for one no archive holds.
    pub(crate) road_textures: &'a [Option<PackageTexture>],
}

/// The terrain's surface, rough, of no metal or specular, as the import's materials are
/// (09-zero-hour.md, A W3D material becomes the one material).
const SURFACE: TerrainSurface = TerrainSurface {
    metallic: 0.0,
    roughness: 1.0,
    specular: 0.0,
};

/// A cell of the heightmap, 10 units wide (`MAP_XY_FACTOR`).
const CELL: i64 = 10;

/// A cell of the partition, 40 units wide (`GameData.ini`'s `PartitionCellSize`), whose highest
/// samples a shell's curve rises over (04-capabilities/00-overview.md, The sim's ground is the
/// map's height grid).
const PARTITION_CELL: i64 = 40;

/// A cell of the pathfinder, 10 units wide (`PATHFIND_CELL_SIZE`), which the navigation's cells
/// take (09-zero-hour.md, Cliffs are walls).
const PATHFIND_CELL: i64 = 10;

/// A step of height, 0.625 units (`MAP_HEIGHT_SCALE`): 5 · 2⁻³, exact in a `Num`.
const STEP: Num = Num::from_bits(5 << (Num::FRAC_BITS - 3));

impl MapImport {
    /// The files of `file`, of `sources`: its heights on the engine's axes, its terrain over them
    /// as a terrain file, its texture the atlas the game builds of its classes; its objects as a
    /// multiplayer game spawns them (09-zero-hour.md, The multiplayer rule for client objects),
    /// none of a template no INI object names, an optimized tree, a prop and fluff as client
    /// objects, of a look of `object_looks`, or none when it has none, and every other as a unit of
    /// the type `names` gives it, the bridge-like ones first, in the map's order; bounds that hold
    /// its heights and each unit's body whole; its waypoints within them as markers, and the names
    /// of those past them, which it drops; the walls of its cliffs within them (09-zero-hour.md,
    /// Cliffs are walls); and its first `lights` object lights, as the game draws
    /// `NumberGlobalLights` of each, which the terrain is lit by too; their ambient color is the
    /// first light's, as the game takes it; and its roads as the terrain's decals, after its
    /// overlays.
    pub(crate) fn new(file: &ZeroHourMap, sources: MapSources<'_, '_>) -> Result<Self, MapError> {
        let MapSources {
            rules,
            names,
            object_looks,
            lights,
            game,
            textures,
            roads,
            road_textures,
        } = sources;
        let ini = &rules.objects;
        let heights = Self::heights(&file.heights)?;
        // The ground a match loads, checked as its load checks it and not built, so a map the load
        // would refuse is refused here, not by the match.
        let ground = Self::ground(&file.heights);
        Ground::check(&heights, ground.high).map_err(MapError::Ground)?;
        let map_lights = MapLights::new(
            file.lights[0].ambient,
            file.lights[..lights].iter().map(|light| MapLight {
                color: light.diffuse,
                direction: Vec3::from_array(axes::y_up(light.position)),
            }),
        )
        .ok_or(MapError::LightDirection)?;
        let terrain = &file.terrain;
        let atlas = TerrainAtlas::new(terrain.fields(), textures, road_textures);
        let cells = TerrainCells::new(terrain.fields(), &file.heights, &atlas, game);
        let untextured = cells.untextured();
        let quads = cells.quads();
        let road_ground = RoadGround {
            heights: &file.heights,
            quads: &quads,
        };
        let drawn = RoadBuffer::of(roads).draw(&rules.roads, road_ground, atlas.roads());
        let terrain = TerrainFile::new(TerrainFileFields {
            quads,
            decals: drawn.decals.into_decals(),
            texture: atlas.into_texture(),
            surface: SURFACE,
        })
        .map_err(MapError::TerrainFile)?;
        let mut low = heights.origin();
        let mut high = heights.far_corner();
        let mut take = |point: [Num; 2]| {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
            point
        };
        let neutral = ImportTeam::Neutral.name();
        let mut bridges = Vec::new();
        let mut units = Vec::new();
        let mut client = ClientObjects::default();
        let mut waypoints = Vec::new();
        let mut ids = BTreeSet::new();
        for (at, object) in file.objects.iter().enumerate() {
            if object.is_road_or_bridge() {
                continue;
            }
            if let Some(waypoint) = &object.waypoint {
                let position = object.ground().and_then(axes::ground);
                let position = position.ok_or(MapError::Placement(at))?;
                let id = u32::try_from(waypoint.id)
                    .ok()
                    .ok_or(MapError::WaypointId(waypoint.id))?;
                if !ids.insert(id) {
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
                waypoints.push(Waypoint { id, name, position });
            } else {
                let Some(template) = ini.object(&object.template) else {
                    continue;
                };
                let placement = object.placement();
                let ObjectPlace {
                    ground,
                    height,
                    degrees,
                } = placement.ok_or(MapError::Placement(at))?;
                let ground = axes::ground(ground).ok_or(MapError::Placement(at))?;
                let placed = template.placed_as();
                if matches!(placed, PlacedAs::Tree | PlacedAs::Prop) {
                    let look = DeclaredName::from(names.take(template)?);
                    if object_looks.contains_key(&look) {
                        client.objects.push(ClientObject {
                            look,
                            position: ground,
                            height,
                            angle: degrees,
                        });
                    }
                    continue;
                }
                let typed = |error| MapError::UnitType {
                    object: template.name.clone(),
                    error,
                };
                // A variation stands as its parent's type at its place, of its own body
                // (09-zero-hour.md, A chassis converts to the parent of its file).
                let variation = rules.variations.placed(template);
                let of_type = variation.map_or(template, |placed| &rules.objects[placed.parent]);
                let placed_type = names.unit_type(of_type, rules).map_err(typed)?;
                let own;
                let body = match variation {
                    Some(_) => {
                        own = ImportedType::of(template, rules).map_err(typed)?;
                        &own
                    }
                    None => placed_type.imported,
                };
                // The bounds hold its whole body, as a placed box's load needs.
                if let Some(reach) = body.reach(degrees) {
                    for corner in [[-1, -1], [1, 1]] {
                        take([0, 1].map(|axis| ground[axis] + reach[axis] * corner[axis]));
                    }
                }
                let unit = PlacedUnitData {
                    unit_type: placed_type.name.declared().clone(),
                    team: neutral.clone(),
                    position: MapPoint::Ground(take(ground)),
                    angle: degrees,
                    height,
                    path: None,
                    from: None,
                    variation: variation.map(|placed| placed.place),
                };
                match placed {
                    PlacedAs::Bridge => bridges.push(unit),
                    _ => units.push(unit),
                }
            }
        }
        bridges.append(&mut units);
        let bounds = Bounds::new(low, high).ok_or(MapError::Bounds)?;
        // A waypoint past the heights and the placed bodies marks no place of the map, as a
        // camera's point far off does: it is dropped, and reported.
        // The original lists a map as a skirmish's of as many players as it has starts from
        // `Player_1_Start` on in a row, two or more (`WaypointMap::update`), and starts each
        // player at its slot's: those are its markers of `start`, and their rallies its markers
        // of `rally`, each with its player slot as scripts count it, from 0.
        let starts = (1..=SLOTS).take_while(|&n| {
            let start = Some(SkirmishPoint::Start(n));
            waypoints
                .iter()
                .any(|waypoint| waypoint.skirmish() == start)
        });
        let slots = starts.last().filter(|&slots| slots >= 2).unwrap_or(0);
        let mut markers = Vec::new();
        let mut dropped_waypoints = Vec::new();
        let rule_names = &rules.names;
        for waypoint in waypoints {
            let point = waypoint.skirmish().filter(|point| point.slot() <= slots);
            let Waypoint { id, name, position } = waypoint;
            if bounds.clamp_ground(position) != position {
                dropped_waypoints.push(name);
                continue;
            }
            let mut tags = vec![rule_names.waypoint.clone()];
            let mut params =
                BTreeMap::from([(rule_names.waypoint_name.clone(), ModeParam::Text(name))]);
            if let Some(point) = point {
                tags.push(match point {
                    SkirmishPoint::Start(_) => rule_names.start.clone(),
                    SkirmishPoint::Rally(_) => rule_names.rally.clone(),
                });
                let player_slot = Scalar::Int(i64::from(point.slot() - 1));
                params.insert(
                    rule_names.player_slot.clone(),
                    ModeParam::Value(player_slot),
                );
            }
            markers.push(MarkerData {
                name: DeclaredName::new(&format!("w{id}")).expect("a `w` and digits are a name"),
                tags,
                position: Some(MapPoint::Ground(position)),
                region: None,
                team: None,
                params,
            });
        }
        let cliffs: Vec<bool> = file
            .terrain
            .fields()
            .cells
            .iter()
            .map(|cell| cell.cliff)
            .collect();
        let frame = Self::frame(&file.heights);
        let walls = CliffWalls::of(&cliffs, file.heights.width).walls(frame, bounds);
        // A planar map of its bounds, its ground, its navigation and its grid of the partition's
        // cells, which the shroud is drawn at, shrouded as `UseShroud` says a skirmish starts
        // (09-zero-hour.md, Zero Hour sees by the vision capability), the sections it gives, then
        // the units and markers of the mode's own.
        let mut map = SectionWriter::default();
        map.section(&BoundsData(bounds))
            .section(&ground)
            .section(&GridData {
                cell: Num::int(PARTITION_CELL),
                brush: Vec::new(),
                shroud: rules.multiplayer.uses_shroud(),
                neutral_in_fog: true,
            })
            .section(&MapNavigationData {
                cell: Num::int(PATHFIND_CELL),
                walls,
            })
            .own::<MapData>(&MapPatch {
                units: (!bridges.is_empty()).then_some(bridges),
                markers: (!markers.is_empty()).then_some(markers),
            });
        Ok(Self {
            map: Toml::write(&map.into_table()),
            heights: Binary::encode(&heights),
            terrain: terrain.encode(),
            untextured,
            road_pieces: drawn.pieces,
            dropped_roads: drawn.dropped,
            lights: map_lights.encode(),
            objects: client.encode(),
            conditions: MapConditions {
                conditions: file.conditions.names(),
            }
            .encode(),
            dropped_waypoints,
        })
    }

    /// The ground as the original's logic reads it (04-capabilities/00-overview.md, The sim's
    /// ground is the map's height grid): each cell split on its own corners 0 to 2, the file's 1 to
    /// 3 (`BaseHeightMapRenderObjClass::getHeightMapHeight`), and the partition's cells from its
    /// world's corner over its first boundary, the playable area, an extent below 1 unit taking 1
    /// (`PartitionManager::init`), its `y` the engine's `−z`.
    fn ground(heights: &MapHeights) -> GroundData {
        let extent = |cells: i32| {
            if cells >= 1 {
                Num::int(i64::from(cells) * CELL)
            } else {
                Num::ONE
            }
        };
        let [x, y] = heights.boundary.unwrap_or([0, 0]);
        GroundData {
            split: CellSplit::OneThree,
            high: Some(CoarseSpan {
                cell: Num::int(PARTITION_CELL),
                from: [Num::ZERO; 2],
                to: [extent(x), -extent(y)],
            }),
        }
    }

    /// Where the samples of `heights` lie: the original's sample of column `i` and row `j` at
    /// `x = (i − border) · 10` and `y = (j − border) · 10`.
    const fn frame(heights: &MapHeights) -> CellFrame {
        CellFrame {
            border: heights.border,
            cell: Num::int(CELL),
        }
    }

    /// The heights on the engine's axes, whose `z` runs back along the game's rows: so the
    /// engine's rows run the file's in reverse, from its last row's.
    fn heights(heights: &MapHeights) -> Result<HeightGrid, MapError> {
        let MapHeights {
            width,
            depth,
            border,
            samples,
            ..
        } = heights;
        if border >= width || border >= depth {
            return Err(MapError::Heights);
        }
        // The grid's first sample is the file's of column 0 and its last row.
        let origin = Self::frame(heights).sample([0, depth - 1]);
        let origin = origin.ok_or(MapError::Heights)?;
        HeightGrid::new(
            origin,
            Num::int(CELL),
            STEP,
            u32::try_from(*width).ok().ok_or(MapError::Heights)?,
            Self::flipped(samples, *width),
        )
        .ok_or(MapError::Heights)
    }

    /// `rows`, each `width` entries, in reverse.
    fn flipped<T: Copy>(rows: &[T], width: usize) -> Vec<T> {
        rows.rchunks_exact(width).flatten().copied().collect()
    }
}

#[cfg(test)]
mod tests;
