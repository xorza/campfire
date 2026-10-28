use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::path::Path;

use bevy_math::UVec2;
use campfire_client_files::{
    ButtonLook, CameraFile, ClientFile, ClientObjects, ClientUnits, EffectsFile, FogFile, FogLevel,
    HitMark, HudFile, KeysFile, MapConditions, MapLights, Minimap, PanelGrid, PanelIcon,
    PanelPlaces, PerkPage, PerkPlace, PerksFile, PickingFile, PixelRadius, PlaceOrder, RadarShown,
    ShownBars, SoundFile, TeamColors, TerrainFile,
};
use campfire_codec::Toml;
use campfire_common::{DeclaredName, MapName, PackageFingerprint};
use campfire_math::Num;
use campfire_package::{MapFile, ModeFile, ModeRead, UnitsFile};
use campfire_package_disk::{PackageDir, PackageWriter, WriteError};
use campfire_package_format::{FileIndex, PackageFiles};
use campfire_store::DurableDir;
use campfire_units::HeightGrid;
use campfire_values::{PackagePath, Speed};
use serde::{Deserialize, Serialize};

use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::{TgaAlpha, TgaFile, TgaHeader};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::chassis_import::ChassisImport;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::contain_import::ContainImport;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::effects_import::{EffectSources, EffectsImport, WeaponShot};
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::RadarPriority;
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::map::map_file::ZeroHourMap;
use crate::zero_hour::format::map::terrain::TextureClass;
use crate::zero_hour::game_time::FRAMES_PER_SECOND;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_loss::ImportLoss;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::import_team::SLOTS;
use crate::zero_hour::install::{Install, TextureKind};
use crate::zero_hour::look_flags::LookFlags;
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::map_roads::MapRoads;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_writer::{ModelReport, ModelWriter, WrittenModel};
use crate::zero_hour::panel_import::PanelImport;
use crate::zero_hour::production_import::ProductionImport;
use crate::zero_hour::road_pieces::RoadPieces;
use crate::zero_hour::rules_package::{FACTION, ModeImport, RULES_FILES};
use crate::zero_hour::science_import::ScienceImport;
use crate::zero_hour::skirmish_start::SkirmishStart;
use crate::zero_hour::sound_import::SoundImport;
use crate::zero_hour::terrain_atlas::PackageTexture;
use crate::zero_hour::unit_types::TypeAdditions;
use crate::zero_hour::upgrade_import::UpgradeImport;
use crate::zero_hour::veterancy_import::VeterancyImport;
use crate::zero_hour::wreck_import::WreckImport;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;
use campfire_client_files::{SpeedScale, TrackMarks, TreadDust, WreckGround};

mod airfield_bones;
pub(crate) mod animation_import;
pub(crate) mod archive_path;
pub(crate) mod area_write;
pub(crate) mod axes;
pub(crate) mod barrel_recoil;
pub(crate) mod camera_import;
pub(crate) mod chassis_import;
pub(crate) mod cliff_walls;
pub(crate) mod combat_import;
pub(crate) mod contain_drop_cause;
pub(crate) mod contain_import;
pub(crate) mod convex_clip;
pub(crate) mod draw_sets;
pub(crate) mod dropped;
pub(crate) mod dropped_roads;
pub(crate) mod effect_drop_cause;
pub(crate) mod effects_import;
pub(crate) mod error;
pub(crate) mod format;
pub(crate) mod game_path;
pub(crate) mod game_time;
pub(crate) mod game_version;
pub(crate) mod import_loss;
pub(crate) mod import_name;
pub(crate) mod import_names;
pub(crate) mod import_team;
pub(crate) mod imported_type;
pub(crate) mod install;
pub(crate) mod look_flags;
pub(crate) mod map_import;
pub(crate) mod map_roads;
pub(crate) mod model_assets;
pub(crate) mod model_bones;
pub(crate) mod model_import;
pub(crate) mod model_nodes;
pub(crate) mod model_writer;
pub(crate) mod object_flight;
pub(crate) mod panel_drop_cause;
pub(crate) mod panel_import;
pub(crate) mod pivot_nodes;
pub(crate) mod power_drop_cause;
mod press_import;
pub(crate) mod production_import;
pub(crate) mod road_buffer;
pub(crate) mod road_decals;
pub(crate) mod road_mesh;
pub(crate) mod road_pieces;
pub(crate) mod rules_names;
pub(crate) mod rules_package;
pub(crate) mod science_import;
pub(crate) mod skirmish_start;
pub(crate) mod sound_drop_cause;
pub(crate) mod sound_import;
pub(crate) mod supply_import;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod type_name;
pub(crate) mod unit_types;
pub(crate) mod upgrade_import;
pub(crate) mod veterancy_import;
pub(crate) mod weapon_drop_cause;
pub(crate) mod wreck_import;
pub(crate) mod zero_hour_rules;

/// The player template of the neutral player, whose color the neutral team takes.
const CIVILIAN: &str = "FactionCivilian";
/// What an import wrote: the packages' fingerprints, each map, by name, which loads, the road
/// pieces each map draws, and what it does not hold of the install, in the order the import met
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: PackageFingerprint,
    /// The rules package's, beside it.
    pub rules: PackageFingerprint,
    pub maps: Vec<MapName>,
    pub roads: BTreeMap<MapName, RoadPieces>,
    pub losses: Vec<ImportLoss>,
}

/// A Zero Hour install, read as the game reads it and of a version the importer knows, which it
/// imports into a package.
#[derive(Debug)]
pub struct ZeroHour {
    install: Install,
    version: &'static GameVersion,
}

impl ZeroHour {
    /// The install under `root`, refused unless its archives are a version the importer knows.
    pub fn open(root: &Path) -> Result<Self, ZeroHourError> {
        Self::open_among(root, GameVersion::KNOWN)
    }

    /// The install under `root`, refused unless its archives are a version of `known`.
    fn open_among(root: &Path, known: &'static [GameVersion]) -> Result<Self, ZeroHourError> {
        let install = Install::open(root)?;
        let version = GameVersion::identify(&install.hashed()?, known)?;
        Ok(Self { install, version })
    }

    /// The version of the install.
    pub const fn version(&self) -> &'static GameVersion {
        self.version
    }

    /// Writes the new directory `out` of two packages: the rules package as the importer ships
    /// it, `zero-hour/`, and the install's, `game/`, a mode that depends on it; their
    /// fingerprints. Each map the game lists becomes `map/<name>/` and `client/maps/<name>/` of
    /// the install's, its name its directory's, and every template its objects name a unit type of
    /// `data/units.toml`; each texture becomes a KTX2 file of `client/textures/`, at its path in
    /// the archives.
    pub fn write(&self, out: &Path) -> Result<Imported, ZeroHourError> {
        DurableDir::create_new(out).map_err(WriteError::CreateDir)?;
        let rules = Self::write_rules(&out.join(Self::RULES))?;
        let mut writer = PackageWriter::create(&out.join(Self::GAME))?;
        let game = ZeroHourRules::read(&self.install)?;
        let camera = camera_import::camera_file(&game.game_data).ok_or(ZeroHourError::Camera)?;
        writer.write(CameraFile::path(), &camera.encode())?;
        let keys = press_import::keys_file(&game.mouse);
        writer.write(KeysFile::path(), &keys.encode())?;
        // Units before structures, each widened to a radius of at least 10 pixels, where the game
        // picks the nearest triangle alone (09-zero-hour.md, Zero Hour's units come before its
        // structures).
        let picking = PickingFile {
            tiers: vec![game.names.no_structures.clone()],
            least_radius_px: PixelRadius::new(10.0).expect("a radius at least 0"),
        };
        writer.write(PickingFile::path(), &picking.encode())?;
        // The players' colors, each team's by its index, the slots' of the multiplayer colors in
        // turn, then the neutral team's of the civilians' template, as the game colors the neutral
        // player by `FactionCivilian`'s `PreferredColor`, white when it gives none
        // (02-engine-core.md, Team colors in the one material).
        let players = game.multiplayer.colors();
        let civilian = game
            .templates
            .named(CIVILIAN)
            .and_then(|civilian| civilian.color);
        let slots = players.iter().cycle().take(usize::from(SLOTS)).copied();
        let colors = slots.chain([civilian.unwrap_or([u8::MAX; 3])]);
        let colors = TeamColors::new(colors.collect())?;
        writer.write(TeamColors::path(), &colors.encode())?;
        let mut models = ModelWriter::new(ModelAssets::new(&self.install)?, &mut writer);
        let mut object_names = ImportNames::default();
        let flags = LookFlags::of(&game.game_data, &game.names, &game.objects)?;
        let mut looks = models.units(&game.objects, &mut object_names, &flags)?;
        let wrecks = WreckImport::new(
            &game.objects,
            &game.creation_lists,
            &game.locomotors,
            game.game_data.gravity,
        )?;
        let wreck_losses = wrecks.name(
            &mut |model: &str| Ok(matches!(models.model(model)?, WrittenModel::Nodes { .. })),
            &mut looks.units,
        )?;
        ChassisImport::name(&game.objects, &game.locomotors, &mut looks.units)?;
        let ModelReport {
            skipped,
            dropped_states,
            dropped_animations,
            missing_textures,
            textures: model_textures,
        } = models.report();
        let variations = game.objects.variations().dropped().into_iter().cloned();
        let mut losses: Vec<ImportLoss> = variations.map(ImportLoss::Variation).collect();
        losses.extend(skipped.into_iter().map(ImportLoss::Model));
        losses.extend(dropped_animations.into_iter().map(ImportLoss::Animation));
        losses.extend(missing_textures.into_iter().map(ImportLoss::Texture));
        losses.extend(dropped_states.into_iter().map(ImportLoss::States));
        let mut names = BTreeMap::<MapName, ArchivePath>::new();
        let mut maps = Vec::new();
        let mut roads = BTreeMap::new();
        let mut terrain_textures = BTreeMap::new();
        let road_assets = ModelAssets::new(&self.install)?;
        for map in self.install.maps() {
            let name = ImportName::of(map.dir.as_bytes()).map()?;
            if let Some(first) = names.insert(name.clone(), map.path.clone()) {
                return Err(ZeroHourError::MapNameClash {
                    first,
                    second: map.path,
                });
            }
            let bytes = self.install.read(&map.path)?;
            let in_map = |error| ZeroHourError::Map {
                path: map.path.clone(),
                error,
            };
            let file = ZeroHourMap::read(&bytes).map_err(|error| in_map(error.into()))?;
            let textures = file
                .terrain
                .fields()
                .classes
                .iter()
                .map(|class| self.class_texture(class, &game.terrain, &mut terrain_textures))
                .collect::<Result<Vec<_>, _>>()?;
            let map_roads = MapRoads::of(&file.objects, &game.roads);
            let road_textures = map_roads
                .types()
                .iter()
                .map(|&road| {
                    let name = &game.roads[road].texture;
                    self.road_texture(name, &road_assets, &mut terrain_textures)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let sources = MapSources {
                rules: &game,
                names: &mut object_names,
                object_looks: &looks.objects,
                lights: game.game_data.global_lights(),
                game: game.game_data.terrain,
                textures: &textures,
                roads: &map_roads,
                road_textures: &road_textures,
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            writer.write(MapFile::path(&name), imported.map.as_bytes())?;
            writer.write(HeightGrid::path(&name), &imported.heights)?;
            writer.write(TerrainFile::path(&name), &imported.terrain)?;
            writer.write(MapConditions::path(&name), &imported.conditions)?;
            writer.write(MapLights::path(&name), &imported.lights)?;
            writer.write(ClientObjects::path(&name), &imported.objects)?;
            if imported.untextured > 0 {
                losses.push(ImportLoss::UntexturedCells {
                    map: name.clone(),
                    cells: imported.untextured,
                });
            }
            let waypoints = imported.dropped_waypoints.into_iter();
            losses.extend(waypoints.map(|waypoint| ImportLoss::Waypoint {
                map: name.clone(),
                waypoint,
            }));
            let dropped_roads = imported.dropped_roads.into_iter();
            losses.extend(dropped_roads.map(|dropped| ImportLoss::Road {
                map: name.clone(),
                dropped,
            }));
            roads.insert(name.clone(), imported.road_pieces);
            maps.push(name);
        }
        let start = SkirmishStart::of(&game, &mut object_names)?;
        let money = game.multiplayer.money()?;
        let mut object_names = object_names.into_production();
        let production = ProductionImport::of(
            &game,
            &mut object_names,
            looks.helipads.clone(),
            looks.airfields.clone(),
        );
        // The panel's icons and keys, its grid, and the atlases its icons draw from
        // (02-engine-core.md, A button's look is its action's).
        let mut panel_dropped = Losses::default();
        let mut panel = PanelImport::of(
            &production.panel,
            &game.images,
            |name| road_assets.texture(name),
            &mut panel_dropped,
        )?;
        // The panel of perks: each faction's purchase sets, each at its places on the game's
        // grid of 5 columns of 3, down each column, its buttons' icons (generals.md, The perk
        // panel by a choice).
        let mut pages = BTreeMap::new();
        for (faction, faces) in ScienceImport::pages(&game)? {
            let mut held = Vec::with_capacity(faces.len());
            for page in faces {
                let mut places = Vec::with_capacity(page.len());
                for face in page {
                    let found = PanelImport::found(
                        &game.images,
                        |name| road_assets.texture(name),
                        face.image.as_deref(),
                    );
                    let icon = match found {
                        Ok((path, rect)) => {
                            let texture = game_path::texture(&path)?;
                            panel.textures.insert(path);
                            Some(PanelIcon { texture, rect })
                        }
                        Err(cause) => {
                            panel_dropped.drop(face.perk.as_str(), cause);
                            None
                        }
                    };
                    places.push(PerkPlace {
                        place: face.place,
                        perk: face.perk,
                        icon,
                    });
                }
                held.push(PerkPage { places });
            }
            pages.insert(faction, held);
        }
        let perks = PerksFile {
            choice: DeclaredName::new(FACTION).expect("the faction's choice is a declared name"),
            grid: PanelGrid {
                columns: NonZeroU8::new(5).expect("5 columns"),
                rows: NonZeroU8::new(3).expect("3 rows"),
                order: PlaceOrder::Columns,
                commands: BTreeMap::new(),
            },
            pages,
        };
        writer.write(PerksFile::path(), &perks.encode())?;
        for (producer, places) in &production.panel.places {
            if let Some(look) = looks.units.get_mut(producer.declared()) {
                look.panel = PanelPlaces::try_from(places.clone())
                    .expect("a command set gives each place one button");
            }
        }
        // The game draws a bar only over a unit selected or under the cursor
        // (`Drawable::drawHealthBar`), and marks no hit (02-engine-core.md, The mode chooses its
        // hit mark).
        // The radar the game draws by a `RadarUpgrade`'s tag, over its `RADAR_CELL_WIDTH` of 128,
        // darkened by `interpolateColorForHeight`'s 0.6, its objects of 2 × 2 texels
        // (02-engine-core.md, A minimap by a radar tag).
        let hud = HudFile {
            bars: ShownBars::Selected,
            hit_mark: HitMark::Hidden,
            panel: panel.grid.clone(),
            minimap: Minimap {
                radar: Some(game.names.radar.clone()),
                ..Minimap::default()
            },
            level_icons: panel.level_icons.clone(),
        };
        writer.write(HudFile::path(), &hud.encode())?;
        let mut object_names = object_names.into_projectiles();
        let combat = CombatImport::of(&game, &mut object_names)?;
        // The systems the projectiles of the weapons the import converts carry.
        let mut trails = Vec::new();
        for (name, weapon) in game.weapons.weapons() {
            let fired = combat.actions.get(&ImportName::latin(name).id("weapon")?);
            if fired.is_some_and(|fired| fired.delivery.is_some()) {
                trails.extend(weapon.exhaust.as_deref());
            }
        }
        // And the systems the treads throw, which a look names as it names a trail
        // (02-engine-core.md, A tread throws dust while it drives).
        let treads = game.objects.objects().flat_map(IniObject::draws);
        trails.extend(treads.flat_map(|draw| draw.tread_dust.iter().flatten().map(String::as_str)));
        // Each sound a weapon, an effect or a look names, its files read once and held until the
        // package is written (09-zero-hour.md, A sound is PCM, the music the game's MP3).
        let language = self.install.language()?;
        let install = &self.install;
        let mut sounds = SoundImport::new(&game.audio, language.as_deref(), |path| {
            if install.contains(path) {
                install.read(path).map(Some)
            } else {
                Ok(None)
            }
        });
        let sources = EffectSources {
            systems: &game.particles,
            lists: &game.effects,
            trails: &trails,
            cap: game.game_data.max_particles,
            texture: |name: &str| road_assets.texture(name),
        };
        let mut effects = EffectsImport::of(sources, &mut sounds)?;
        // The wrecks fall by the game's gravity and meet its ground (02-engine-core.md, Wrecks move
        // by the client's step): an object's friction on the ground, `DEFAULT_FORWARD_FRICTION` and
        // `DEFAULT_LATERAL_FRICTION`, takes 0.15 of its speed a frame, and it rests below
        // `STUN_RELIEF_EPSILON`, half a world unit a frame.
        let ground = WreckGround {
            gravity: game.game_data.gravity,
            stiffness: game.game_data.ground_stiffness,
            friction: 0.15,
            rest: 0.5 * FRAMES_PER_SECOND,
        };
        effects.file = effects.file.clone().with_wrecks(ground)?;
        // A tread's dust runs while its unit moves faster than the game's `DEBRIS_THRESHOLD`, a
        // squared speed of 0.00001 a frame, scaled across by half its speed a frame and a tenth,
        // and up and in count by its speed a frame and a tenth, each up to 1
        // (`W3DTankDraw::doDrawModule`). A track lays an edge each cell of 10 it moves, 0.2 of a
        // cell above the ground, up to `GlobalData`'s 100 edges, of which the newest 25 stay
        // opaque, each fading over 300 s (`W3DModelDraw`, `W3DTerrainTracks.cpp`), when the game
        // makes track marks.
        let per_frame = |scale: f32| scale / FRAMES_PER_SECOND;
        let dust = TreadDust {
            moving: 0.00001_f32.sqrt() * FRAMES_PER_SECOND,
            across: SpeedScale {
                base: 0.1,
                per_speed: per_frame(0.5),
                most: 1.0,
            },
            rise: SpeedScale {
                base: 0.1,
                per_speed: per_frame(1.0),
                most: 1.0,
            },
            count: SpeedScale {
                base: 0.1,
                per_speed: per_frame(1.0),
                most: 1.0,
            },
        };
        let tracks = if game.game_data.track_marks {
            TrackMarks {
                spacing: 10.0,
                lift: 0.2 * 10.0,
                edges: 100,
                opaque: 25,
                fade: 300.0,
                tracks: game.game_data.most_tracks,
            }
        } else {
            TrackMarks::NONE
        };
        effects.file = effects.file.clone().with_treads(dust, tracks)?;
        effects.name_deaths(
            &game.objects,
            |name: &str| road_assets.texture(name),
            &mut looks,
        )?;
        // The textures the models, the terrain and the effects draw with, each converted once: no
        // other file of the install is drawn.
        let drawn = model_textures
            .into_iter()
            .chain(terrain_textures.keys().cloned())
            .chain(effects.textures.iter().cloned())
            .chain(panel.textures.iter().cloned());
        for (to, path) in texture_files(drawn)? {
            let kind = TextureKind::of(&path).expect("a drawn texture is DDS or TGA");
            // A terrain's texture was read for its size, and is not read again.
            let bytes = match terrain_textures.remove(&path) {
                Some(read) => read.bytes,
                None => self.install.read(&path)?,
            };
            let texture = |error| ZeroHourError::Texture {
                path: path.clone(),
                error,
            };
            let ktx2 = match kind {
                TextureKind::Dds => DdsFile::read(&bytes).map_err(texture)?.ktx2(),
                // Zero Hour's loader keeps a texel's alpha whatever the header counts.
                TextureKind::Tga => TgaFile::read(&bytes, TgaAlpha::Always)
                    .map_err(texture)?
                    .ktx2(),
            };
            writer.write(to, &ktx2)?;
        }
        losses.extend(effects.dropped.iter().cloned().map(ImportLoss::Effect));
        losses.extend(wreck_losses.into_iter().map(ImportLoss::Effect));
        combat.complete_looks(&mut looks.units);
        let delivery = |action: &DeclaredName| match combat.actions.get(action) {
            None => WeaponShot::Dropped,
            Some(weapon) => weapon
                .delivery
                .as_ref()
                .map_or(WeaponShot::Shot, |delivery| {
                    WeaponShot::Projectile(&delivery.projectile)
                }),
        };
        let mut actions =
            effects.action_looks(&game.weapons, delivery, &mut sounds, &mut looks.units)?;
        // Each look's voices, and who sees it on the minimap, and the music of each faction's base
        // side (02-engine-core.md, A look names its voices; 02-engine-core.md, The minimap is drawn
        // from the terrain at load; 02-engine-core.md, Music by a choice).
        for object in game.objects.objects() {
            let declared = ImportName::object(object).declared()?;
            if let Some(look) = looks.units.get_mut(&declared) {
                look.voices = sounds.voices(object.voices())?;
                look.radar = match object.radar_priority() {
                    RadarPriority::Unset | RadarPriority::NotOnRadar => RadarShown::Never,
                    RadarPriority::LocalUnitOnly => RadarShown::Owner,
                    RadarPriority::Shown => RadarShown::Everyone,
                };
            }
        }
        let factions = start.factions.iter().filter_map(|faction| {
            let side = faction.base_side.as_deref()?;
            Some((faction.name.clone(), side))
        });
        let choice = DeclaredName::new(FACTION).expect("the faction's choice is a declared name");
        let music = sounds.music(choice, factions)?;
        let sound_file = sounds.sound_file(Some(music));
        let sounded = sounds.finish();
        effects.file = effects.file.clone().with_sounds(sounded.events)?;
        writer.write(EffectsFile::path(), &effects.file.encode())?;
        writer.write(SoundFile::path(), &sound_file.encode())?;
        // The fog of the game's shroud levels (02-engine-core.md, The fog is the one material's).
        let game_data = &game.game_data;
        let fog = FogFile::new(
            game_data.shroud_levels.map(FogLevel),
            game_data.shroud_color,
        )?;
        writer.write(FogFile::path(), &fog.encode())?;
        for (to, bytes) in &sounded.files {
            writer.write(to.clone(), bytes)?;
        }
        losses.extend(sounded.dropped.into_iter().map(ImportLoss::Sound));
        for (action, button) in panel.actions {
            if button == ButtonLook::default() {
                continue;
            }
            let look = actions.entry(action).or_default();
            look.icon = button.icon;
            look.key = button.key;
        }
        let looks_bones = looks.bones;
        let fire_points = std::mem::take(&mut looks.fire_points);
        let looks =
            ClientUnits::new(looks.units, looks.conditions, looks.objects)?.with_actions(actions);
        let upgrades = UpgradeImport::of(&game, object_names.types(), &production, &combat)?;
        let veterancy = VeterancyImport::of(&game.game_data, object_names.types(), &game.names)?;
        let needed = production.sciences.iter().chain(&veterancy.sciences);
        let sciences = ScienceImport::of(&game, needed.map(String::as_str))?;
        let contains = ContainImport::of(&game, object_names.types(), &fire_points)?;
        writer.write(ClientUnits::path(), &looks.encode())?;
        // Every unit type is made: the maps', the start's, the command sets' and the projectiles
        // their weapons fire. The stages after read their closed set, and return what they give
        // each.
        let types = object_names.close();
        let retaliation = rules_package::retaliation(game.ai.retaliation, &game.names)?;
        let mode = ModeImport {
            start: &start,
            production: &production,
            combat: &combat,
            upgrades: &upgrades,
            veterancy: &veterancy,
            sciences: &sciences,
            contains: &contains,
            start_money: &money,
            retaliation: retaliation.as_ref(),
        };
        writer.write(ModeFile::path(), rules_package::mode_data(mode).as_bytes())?;
        let barrels = combat.barrels(&looks_bones);
        let additions = TypeAdditions {
            barrels: &barrels,
            bones: &looks_bones,
            weapon: &combat.weapons,
            weapon_slots: &combat.weapon_slots,
            command: &production.commands,
            production: &production.sections,
            sells: &production.sells,
            airfields: &production.airfields,
            nodes: &production.supply.nodes,
            drop_offs: &production.supply.drop_offs,
            spawns: &production.supply.spawns,
            upgrades: &upgrades.types,
            tracks: &veterancy.tracks,
            experience: &veterancy.experience,
            areas: &production.areas,
            enters: &contains.enters,
            rides: &contains.rides,
        };
        let units = types.units_file(additions, &game.names);
        writer.write(UnitsFile::path(), units.text.as_bytes())?;
        // A mode whose types drive none caps no speed it moves at; 1 m a second is as good as any.
        let max_move_speed = units
            .max_speed
            .and_then(Speed::new)
            .unwrap_or(Speed::new(Num::ONE).expect("1 m a second is a speed"));
        let manifest = rules_package::mode_manifest(max_move_speed);
        let text = rules_package::mode_text(&production.texts);
        writer.write(manifest.header.language.text_path(), text.as_bytes())?;
        let manifest = Toml::write(&manifest);
        writer.write(PackageFiles::manifest_path(), manifest.as_bytes())?;
        let fingerprint = writer.finish()?;
        // The engine's own load of each map is the check of what the import wrote, so it
        // repeats none of the load's game.
        let game = PackageDir::new(out.join(Self::GAME));
        let load = |map: &MapName, error| ZeroHourError::Load {
            map: map.clone(),
            error: Box::new(error),
        };
        // A read that fails fails every map's load, so its error names the first.
        if let Some(first) = maps.first() {
            let read = ModeRead::at(&game).map_err(|error| load(first, error))?;
            for map in &maps {
                read.on(Some(map)).map_err(|error| load(map, error))?;
            }
        }
        losses.extend(production.dropped.into_iter().map(ImportLoss::Button));
        losses.extend(production.powers_dropped.into_iter().map(ImportLoss::Power));
        let panel_losses = production.panel.dropped.into_iter().chain(panel_dropped);
        losses.extend(panel_losses.map(ImportLoss::Panel));
        losses.extend(upgrades.dropped.into_iter().map(ImportLoss::Upgrade));
        let supply = production.supply.dropped.into_iter();
        losses.extend(supply.map(ImportLoss::Supply));
        losses.extend(combat.dropped.into_iter().map(ImportLoss::Weapon));
        losses.extend(contains.dropped.into_iter().map(ImportLoss::Contain));
        losses.extend(combat.unliving.into_iter().map(ImportLoss::Unliving));
        losses.extend(combat.wounded.into_iter().map(ImportLoss::Wounded));
        losses.extend(combat.unpreferred.into_iter().map(ImportLoss::Unpreferred));
        losses.extend(
            veterancy
                .untrainable
                .into_iter()
                .map(ImportLoss::Untrainable),
        );
        Ok(Imported {
            fingerprint,
            rules,
            maps,
            roads,
            losses,
        })
    }
}

impl ZeroHour {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = rules_package::RULES;
    pub const GAME: &'static str = "game";

    /// The version of what `write` writes from an install: raised with each change of its output,
    /// as a check keeps an import of the same install and version and writes nothing
    /// (09-zero-hour.md, A check keeps an unchanged import).
    pub const OUTPUT_VERSION: u32 = 71;

    /// Writes the rules package into the new directory `dir`; its fingerprint, which is
    /// `rules_fingerprint`'s.
    fn write_rules(dir: &Path) -> Result<PackageFingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir)?;
        for (path, bytes) in RULES_FILES {
            writer.write(fixed(path), bytes)?;
        }
        let written = writer.finish()?;
        debug_assert_eq!(
            written,
            Self::rules_fingerprint(),
            "the rules' index is its files'"
        );
        Ok(written)
    }

    /// The fingerprint of the rules package this importer writes, read from the index the
    /// workspace keeps of its files, so a kept import of other rules is known without a write
    /// (09-zero-hour.md, A check keeps an unchanged import).
    pub fn rules_fingerprint() -> PackageFingerprint {
        let index = include_bytes!("../../../../../packages/zero-hour/package.index");
        FileIndex::decode(index)
            .expect("the workspace's index of the rules decodes")
            .fingerprint()
    }

    /// The texture of `class`: the TGA `Art\Terrain\` holds by the name its terrain type gives,
    /// as `readTexClass` opens it, read once into `read` with its size, for its conversion; none
    /// for a class no type names, or a TGA the install lacks.
    fn class_texture(
        &self,
        class: &TextureClass,
        types: &TerrainIni,
        read: &mut BTreeMap<ArchivePath, TerrainTexture>,
    ) -> Result<Option<PackageTexture>, ZeroHourError> {
        let Some(file) = types.texture(&class.name) else {
            return Ok(None);
        };
        let path = ArchivePath::of(&format!("art\\terrain\\{file}"));
        if path.extension() != Some("tga") || !self.install.contains(&path) {
            return Ok(None);
        }
        let size = if let Some(texture) = read.get(&path) {
            texture.size
        } else {
            let bytes = self.install.read(&path)?;
            let header = TgaHeader::read(&bytes).map_err(|error| ZeroHourError::Texture {
                path: path.clone(),
                error,
            })?;
            let size = header.size();
            read.insert(path.clone(), TerrainTexture { size, bytes });
            size
        };
        Ok(Some(PackageTexture {
            path: game_path::texture(&path)?,
            size,
        }))
    }

    /// The texture of a road type, of the file `name` its `Texture` gives, as the game's asset
    /// manager finds it: its DDS, else its TGA, in `Art\Textures\` of the language's directory,
    /// then of the archives' (09-zero-hour.md, A road's type is `Roads.ini`'s), read once into
    /// `read` with its size, for its conversion; none for one no archive holds.
    fn road_texture(
        &self,
        name: &str,
        assets: &ModelAssets<'_>,
        read: &mut BTreeMap<ArchivePath, TerrainTexture>,
    ) -> Result<Option<PackageTexture>, ZeroHourError> {
        let Some(path) = assets.texture(name) else {
            return Ok(None);
        };
        let size = if let Some(texture) = read.get(&path) {
            texture.size
        } else {
            let bytes = self.install.read(&path)?;
            let texture = |error| ZeroHourError::Texture {
                path: path.clone(),
                error,
            };
            let size = match TextureKind::of(&path) {
                Some(TextureKind::Dds) => DdsFile::read(&bytes).map_err(texture)?.size(),
                _ => TgaHeader::read(&bytes).map_err(texture)?.size(),
            };
            read.insert(path.clone(), TerrainTexture { size, bytes });
            size
        };
        Ok(Some(PackageTexture {
            path: game_path::texture(&path)?,
            size,
        }))
    }
}

/// A terrain's texture as the import read it for its size: the size, and the file's bytes, which
/// its conversion takes.
#[derive(Debug)]
struct TerrainTexture {
    size: UVec2,
    bytes: Vec<u8>,
}

/// Where each texture of `drawn` goes in the package, each once; an error for two that go to one
/// file, as an `x.dds` and an `x.tga` of one directory would.
fn texture_files(
    drawn: impl IntoIterator<Item = ArchivePath>,
) -> Result<BTreeMap<PackagePath, ArchivePath>, ZeroHourError> {
    let mut files = BTreeMap::<PackagePath, ArchivePath>::new();
    for path in drawn {
        let to = game_path::texture(&path)?;
        match files.get(&to) {
            Some(first) if *first == path => {}
            Some(first) => {
                return Err(ZeroHourError::TextureClash {
                    first: first.clone(),
                    second: path,
                });
            }
            None => {
                files.insert(to, path);
            }
        }
    }
    Ok(files)
}

/// The path `text` of a file the import writes by a name of its own.
fn fixed(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("the import's own paths are package paths")
}

#[cfg(test)]
mod tests;
