use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU8;
use std::path::Path;

use bevy_math::UVec2;
use campfire_client_files::{
    ActionLook, ButtonLook, CameraFile, ClientFile, ClientObjects, ClientUnit, ClientUnits,
    EffectsFile, FogFile, FogLevel, HitMark, HudFile, KeysFile, MapConditions, MapLights, Minimap,
    ObjectLook, PanelGrid, PanelPlaces, PerkPage, PerkPlace, PerksFile, PickingFile, PixelRadius,
    PlaceOrder, RadarShown, ShownBars, SoundFile, SpeedScale, TeamColors, TerrainFile, TrackMarks,
    TreadDust, WreckGround,
};
use campfire_codec::{Toml, TomlValue};
use campfire_common::{DeclaredName, MapName, PackageFingerprint};
use campfire_math::Num;
use campfire_package::{MapFile, ModeFile, UnitsFile};
use campfire_package_disk::PackageWriter;
use campfire_package_format::PackageFiles;
use campfire_units::HeightGrid;
use campfire_values::{PackagePath, Speed};

use crate::texture::dds_file::DdsFile;
use crate::texture::texture_kind::TextureKind;
use crate::texture::tga_file::{TgaAlpha, TgaFile, TgaHeader};
use crate::zero_hour::additions::Additions;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::art_paths::ArtPaths;
use crate::zero_hour::camera_import;
use crate::zero_hour::chassis_import::ChassisImport;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::contain_import::ContainImport;
use crate::zero_hour::dropped::{Dropped, Losses};
use crate::zero_hour::effect_drop_cause::EffectDropCause;
use crate::zero_hour::effects_import::{EffectSources, EffectsImport};
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_frames;
use crate::zero_hour::format::ini::object_ini::ini_object::{IniObject, RadarPriority};
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::map::map_file::ZeroHourMap;
use crate::zero_hour::format::map::terrain::TextureClass;
use crate::zero_hour::game_path;
use crate::zero_hour::import_loss::ImportLoss;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::import_team::SLOTS;
use crate::zero_hour::install::Install;
use crate::zero_hour::look_flags::LookFlags;
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::map_roads::MapRoads;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_converter::{ConvertedModel, WrittenModel};
use crate::zero_hour::model_writer::{ImportedLooks, ModelReport, ModelWriter};
use crate::zero_hour::panel_drop_cause::PanelDropCause;
use crate::zero_hour::panel_import::PanelImport;
use crate::zero_hour::press_import;
use crate::zero_hour::production_cx::ProductionCx;
use crate::zero_hour::production_import::ProductionImport;
use crate::zero_hour::propaganda_import::PropagandaImport;
use crate::zero_hour::road_pieces::RoadPieces;
use crate::zero_hour::rules_package;
use crate::zero_hour::rules_package::{FACTION, ModeImport};
use crate::zero_hour::science_import::ScienceImport;
use crate::zero_hour::skirmish_start::SkirmishStart;
use crate::zero_hour::sound_import::SoundImport;
use crate::zero_hour::terrain_atlas::PackageTexture;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::unit_types::{ImportedTypes, TypeModifiers, WrittenUnits};
use crate::zero_hour::upgrade_import::{UpgradeImport, UpgradeSources};
use crate::zero_hour::veterancy_import::VeterancyImport;
use crate::zero_hour::wreck_import::WreckImport;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The player template of the neutral player, whose color the neutral team takes.
const CIVILIAN: &str = "FactionCivilian";

/// One write of an install's package, stage by stage, each stage's data dropped as it ends: the
/// install and its rules, where its art lies, the package's writer, and what the import does not
/// hold of the install, in the order the import meets it.
#[derive(Debug)]
pub(crate) struct GameWrite<'r> {
    install: &'r Install,
    game: &'r ZeroHourRules,
    art: ArtPaths<'r>,
    writer: PackageWriter,
    losses: Vec<ImportLoss>,
}

/// What a write of the install's package gives: the package's fingerprint, each map, by name, the
/// road pieces each map draws, and what it does not hold of the install.
#[derive(Debug)]
pub(crate) struct GameWritten {
    pub(crate) fingerprint: PackageFingerprint,
    pub(crate) maps: Vec<MapName>,
    pub(crate) roads: BTreeMap<MapName, RoadPieces>,
    pub(crate) losses: Vec<ImportLoss>,
}

/// The units' looks and the types' first names and additions, the models and the animations
/// they draw, converted, and what the wrecks and the chassis drop, which the import meets with
/// the effects'.
#[derive(Debug)]
struct Looked<'r> {
    looks: ImportedLooks,
    names: ImportNames<'r>,
    converted: ConvertedFiles,
    effect_losses: Vec<Dropped<EffectDropCause>>,
}

/// The models and the animations the import converted, by their paths in the package, each
/// written once a look or an effect names it.
#[derive(Debug)]
struct ConvertedFiles {
    models: BTreeMap<PackagePath, ConvertedModel>,
    animations: BTreeMap<PackagePath, Vec<u8>>,
}

/// The maps the import wrote, by name, the road pieces each draws, and the terrain textures they
/// read, each once, for their conversion.
#[derive(Debug)]
struct WrittenMaps {
    maps: Vec<MapName>,
    roads: BTreeMap<MapName, RoadPieces>,
    textures: BTreeMap<ArchivePath, TerrainTexture>,
}

/// The panel's import, and the buttons of the perks and the icons it dropped.
#[derive(Debug)]
struct WrittenPanel {
    import: PanelImport,
    dropped: Losses<PanelDropCause>,
}

/// What the effects and the sounds read beside the looks: the weapons combat converted, the
/// closed unit types, the skirmish's factions, and the additions that hold each type's weapons.
#[derive(Debug, Clone, Copy)]
struct LookSources<'a> {
    combat: &'a CombatImport,
    types: &'a ImportedTypes,
    start: &'a SkirmishStart,
    additions: &'a Additions,
}

/// The effects the import wrote, the textures they draw, and the looks of the actions.
#[derive(Debug)]
struct WrittenEffects {
    file: EffectsFile,
    textures: BTreeSet<ArchivePath>,
    actions: BTreeMap<DeclaredName, ActionLook>,
}

/// A terrain's texture as the import read it for its size: the size, and the file's bytes, which
/// its conversion takes.
#[derive(Debug)]
struct TerrainTexture {
    size: UVec2,
    bytes: Vec<u8>,
}

impl<'r> GameWrite<'r> {
    /// A write of `install`'s package of its rules `game` into the new directory `dir`.
    pub(crate) fn new(
        install: &'r Install,
        game: &'r ZeroHourRules,
        dir: &Path,
    ) -> Result<Self, ZeroHourError> {
        Ok(Self {
            install,
            game,
            art: ArtPaths::new(install),
            writer: PackageWriter::create(dir)?,
            losses: Vec::new(),
        })
    }

    /// Writes the package: each map the game lists becomes `map/<name>/` and
    /// `client/maps/<name>/`, its name its directory's, and every template its objects name a
    /// unit type of `data/units.toml`; each texture becomes a KTX2 file of `client/textures/`, at
    /// its path in the archives.
    pub(crate) fn write(mut self) -> Result<GameWritten, ZeroHourError> {
        let game = self.game;
        self.write_settings()?;
        let Looked {
            mut looks,
            mut names,
            converted,
            effect_losses,
        } = self.look_units()?;
        let mut additions = std::mem::take(&mut looks.additions);
        let maps = self.write_maps(&mut names, &looks.objects)?;
        let start = SkirmishStart::of(game, &mut names)?;
        let money = game.multiplayer.money()?;
        let mut names = names.into_production();
        let cx = ProductionCx {
            rules: game,
            names: &mut names,
            additions: &mut additions,
        };
        let production = ProductionImport::of(cx, looks.points.clone());
        let panel = self.write_panels(&production, &mut looks.units)?;
        let mut names = names.into_projectiles();
        let combat = CombatImport::of(game, &mut names, &mut additions)?;
        let sources = UpgradeSources {
            rules: game,
            production: &production,
            combat: &combat,
        };
        let mut upgrades = UpgradeImport::of(sources, names.types(), &mut additions)?;
        let propaganda = PropagandaImport::of(game, names.types(), &mut additions)?;
        upgrades.take_propaganda(&propaganda);
        let veterancy = VeterancyImport::of(game, names.types(), &mut additions)?;
        let needed = production.sciences.iter().chain(&veterancy.sciences);
        let sciences = ScienceImport::of(game, needed.map(String::as_str))?;
        let contains = ContainImport::of(game, names.types(), &mut additions)?;
        // Every unit type is made: the maps', the start's, the command sets' and the projectiles
        // their weapons fire. The stages after read their closed set.
        let types = names.close();
        combat.barrels(&mut additions);
        let sources = LookSources {
            combat: &combat,
            types: &types,
            start: &start,
            additions: &additions,
        };
        let mut effects = self.write_effects(sources, &mut looks, effect_losses)?;
        for (action, button) in panel.import.actions {
            if button == ButtonLook::default() {
                continue;
            }
            let look = effects.actions.entry(action).or_default();
            look.icon = button.icon;
            look.key = button.key;
        }
        let units = ClientUnits::new(looks.units, looks.conditions, looks.objects)?
            .with_actions(effects.actions);
        self.writer.write(ClientUnits::path(), &units.encode())?;
        let model_textures = self.write_models(&units, &effects.file, converted)?;
        drop(units);
        let drawn = model_textures
            .into_iter()
            .chain(maps.textures.keys().cloned())
            .chain(effects.textures)
            .chain(panel.import.textures);
        let files = Self::texture_files(drawn)?;
        self.write_textures(files, maps.textures)?;
        let retaliation = rules_package::retaliation(game.ai.retaliation, &game.names)?;
        let movers = types.movers();
        let second_lives = types.second_lives(&game.names);
        let mode = ModeImport {
            start: &start,
            additions: &additions,
            production: &production,
            combat: &combat,
            upgrades: &upgrades,
            veterancy: &veterancy,
            sciences: &sciences,
            contains: &contains,
            start_money: &money,
            retaliation: retaliation.as_ref(),
            movers: &movers,
            second_lives: &second_lives,
            propaganda: &propaganda,
        };
        let modifiers = TypeModifiers {
            movers: &movers,
            second_lives: &second_lives,
        };
        let units = types.units_file(&additions, modifiers, &game.names);
        self.write_mode(mode, &units, &production.texts)?;
        let fingerprint = self.writer.finish()?;
        let mut losses = self.losses;
        losses.extend(production.dropped.into_iter().map(ImportLoss::Button));
        losses.extend(production.powers_dropped.into_iter().map(ImportLoss::Power));
        let panel_losses = production.panel.dropped.into_iter().chain(panel.dropped);
        losses.extend(panel_losses.map(ImportLoss::Panel));
        losses.extend(upgrades.dropped.into_iter().map(ImportLoss::Upgrade));
        losses.extend(propaganda.dropped.into_iter().map(ImportLoss::Upgrade));
        let supply = production.supply.dropped.into_iter();
        losses.extend(supply.map(ImportLoss::Supply));
        losses.extend(combat.dropped.into_iter().map(ImportLoss::Weapon));
        losses.extend(contains.dropped.into_iter().map(ImportLoss::Contain));
        losses.extend(combat.unliving.into_iter().map(ImportLoss::Unliving));
        losses.extend(combat.wounded.into_iter().map(ImportLoss::Wounded));
        losses.extend(combat.unpreferred.into_iter().map(ImportLoss::Unpreferred));
        let untrainable = veterancy.untrainable.into_iter();
        losses.extend(untrainable.map(ImportLoss::Untrainable));
        Ok(GameWritten {
            fingerprint,
            maps: maps.maps,
            roads: maps.roads,
            losses,
        })
    }

    /// Writes the camera, the keys, the picking, the players' colors and the fog, which the
    /// game's data gives whatever the units.
    fn write_settings(&mut self) -> Result<(), ZeroHourError> {
        let game = self.game;
        let camera = camera_import::camera_file(&game.game_data).ok_or(ZeroHourError::Camera)?;
        self.writer.write(CameraFile::path(), &camera.encode())?;
        let keys = press_import::keys_file(&game.mouse);
        self.writer.write(KeysFile::path(), &keys.encode())?;
        // Units before structures, each widened to a radius of at least 10 pixels, where the game
        // picks the nearest triangle alone (09-zero-hour.md, Zero Hour's units come before its
        // structures).
        let picking = PickingFile {
            tiers: vec![game.names.no_structures.clone()],
            least_radius_px: PixelRadius::new(10.0).expect("a radius at least 0"),
        };
        self.writer.write(PickingFile::path(), &picking.encode())?;
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
        self.writer.write(TeamColors::path(), &colors.encode())?;
        // The fog of the game's shroud levels (02-engine-core.md, The fog is the one material's).
        let game_data = &game.game_data;
        let fog = FogFile::new(
            game_data.shroud_levels.map(FogLevel),
            game_data.shroud_color,
        )?;
        self.writer.write(FogFile::path(), &fog.encode())?;
        Ok(())
    }

    /// The units' looks, the models they draw, converted, their wrecks and chassis, and the names
    /// of the types the looks take; the variations, the models, the animations and the states it
    /// drops join the losses.
    fn look_units(&mut self) -> Result<Looked<'r>, ZeroHourError> {
        let game = self.game;
        let mut models = ModelWriter::new(ModelAssets::new(self.install));
        let mut names = ImportNames::default();
        let flags = LookFlags::of(&game.game_data, &game.names, &game.objects)?;
        let mut looks = models.units(game, &mut names, &flags)?;
        let wrecks = WreckImport::new(
            &game.objects,
            &game.creation_lists,
            &game.locomotors,
            game.game_data.gravity,
        )?;
        let mut effect_losses = wrecks.name(
            &mut |model: &str| Ok(matches!(models.model(model)?, WrittenModel::Nodes { .. })),
            &mut looks.units,
        )?;
        effect_losses.extend(ChassisImport::name(
            &game.objects,
            &game.locomotors,
            &mut looks.units,
        )?);
        let ModelReport {
            skipped,
            dropped_states,
            dropped_animations,
            models: converted_models,
            animations: converted_animations,
        } = models.report();
        let variations = game.variations.dropped().into_iter().cloned();
        self.losses.extend(variations.map(ImportLoss::Variation));
        self.losses
            .extend(skipped.into_iter().map(ImportLoss::Model));
        let animations = dropped_animations.into_iter();
        self.losses.extend(animations.map(ImportLoss::Animation));
        self.losses
            .extend(dropped_states.into_iter().map(ImportLoss::States));
        Ok(Looked {
            looks,
            names,
            converted: ConvertedFiles {
                models: converted_models,
                animations: converted_animations,
            },
            effect_losses,
        })
    }

    /// Writes each map of the install, of the object looks `objects`, the templates its objects
    /// name joining `names`; the cells, the waypoints and the roads each drops join the losses.
    fn write_maps(
        &mut self,
        names: &mut ImportNames<'r>,
        objects: &BTreeMap<DeclaredName, ObjectLook>,
    ) -> Result<WrittenMaps, ZeroHourError> {
        let game = self.game;
        let mut written = WrittenMaps {
            maps: Vec::new(),
            roads: BTreeMap::new(),
            textures: BTreeMap::new(),
        };
        let mut paths = BTreeMap::<MapName, ArchivePath>::new();
        for map in self.install.maps() {
            let name = ImportName::of(map.dir.as_bytes()).map()?;
            if let Some(first) = paths.insert(name.clone(), map.path.clone()) {
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
                .map(|class| self.class_texture(class, &game.terrain, &mut written.textures))
                .collect::<Result<Vec<_>, _>>()?;
            let map_roads = MapRoads::of(&file.objects, &game.roads);
            let road_textures = map_roads
                .types()
                .iter()
                .map(|&road| self.road_texture(&game.roads[road].texture, &mut written.textures))
                .collect::<Result<Vec<_>, _>>()?;
            let sources = MapSources {
                rules: game,
                names,
                object_looks: objects,
                lights: game.game_data.global_lights(),
                game: game.game_data.terrain,
                textures: &textures,
                roads: &map_roads,
                road_textures: &road_textures,
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            self.writer
                .write(MapFile::path(&name), imported.map.as_bytes())?;
            self.writer
                .write(HeightGrid::path(&name), &imported.heights)?;
            self.writer
                .write(TerrainFile::path(&name), &imported.terrain)?;
            self.writer
                .write(MapConditions::path(&name), &imported.conditions)?;
            self.writer
                .write(MapLights::path(&name), &imported.lights)?;
            self.writer
                .write(ClientObjects::path(&name), &imported.objects)?;
            if imported.untextured > 0 {
                self.losses.push(ImportLoss::UntexturedCells {
                    map: name.clone(),
                    cells: imported.untextured,
                });
            }
            let waypoints = imported.dropped_waypoints.into_iter();
            self.losses
                .extend(waypoints.map(|waypoint| ImportLoss::Waypoint {
                    map: name.clone(),
                    waypoint,
                }));
            let dropped_roads = imported.dropped_roads.into_iter();
            self.losses
                .extend(dropped_roads.map(|dropped| ImportLoss::Road {
                    map: name.clone(),
                    dropped,
                }));
            written.roads.insert(name.clone(), imported.road_pieces);
            written.maps.push(name);
        }
        Ok(written)
    }

    /// Writes the panel of `production`'s buttons, its perks and the hud, and gives each look of
    /// `looks` its buttons' places and its type's icon.
    fn write_panels(
        &mut self,
        production: &ProductionImport,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<WrittenPanel, ZeroHourError> {
        let (game, art) = (self.game, self.art);
        // The panel's icons and keys, its grid, and the atlases its icons draw from
        // (02-engine-core.md, A button's look is its action's).
        let mut dropped = Losses::default();
        let mut panel = PanelImport::of(
            &production.panel,
            &game.images,
            |name| art.texture(name),
            &mut dropped,
        )?;
        // The panel of perks: each faction's purchase sets, each at its places on the game's
        // grid of 5 columns of 3, down each column, its buttons' icons (02-engine-core.md, The perk
        // panel by a choice).
        let mut pages = BTreeMap::new();
        for (faction, faces) in ScienceImport::pages(game)? {
            let mut held = Vec::with_capacity(faces.len());
            for page in faces {
                let mut places = Vec::with_capacity(page.len());
                for face in page {
                    let found = PanelImport::found(
                        &game.images,
                        |name| art.texture(name),
                        face.image.as_deref(),
                    );
                    let icon = panel.icon(found, face.perk.as_str(), &mut dropped)?;
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
        self.writer.write(PerksFile::path(), &perks.encode())?;
        for (producer, places) in &production.panel.places {
            if let Some(look) = looks.get_mut(producer.declared()) {
                look.panel = PanelPlaces::try_from(places.clone())
                    .expect("a command set gives each place one button");
            }
        }
        // Each type's icon, its `ButtonImage`, which a button that stands for its unit shows, as
        // a container's exit shows its rider (02-engine-core.md, The client orders entries by the
        // right click).
        for object in game.objects.objects() {
            let Some(image) = object.button_image() else {
                continue;
            };
            let name = TypeName::of(object)?;
            let Some(look) = looks.get_mut(name.declared()) else {
                continue;
            };
            let found = PanelImport::found(&game.images, |name| art.texture(name), Some(image));
            if let Some(icon) = panel.icon(found, &object.name, &mut dropped)? {
                look.icon = Some(icon);
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
        self.writer.write(HudFile::path(), &hud.encode())?;
        Ok(WrittenPanel {
            import: panel,
            dropped,
        })
    }

    /// Writes the effects and the sounds, of the weapons, the deaths and the treads of
    /// `sources`, and completes `looks`: the projectiles' looks and each type's weapons, its
    /// deaths, its voices and its radar, and the looks of no type go; what the effects drop,
    /// then `model_losses`, then the sounds dropped join the losses.
    fn write_effects(
        &mut self,
        sources: LookSources<'_>,
        looks: &mut ImportedLooks,
        model_losses: Vec<Dropped<EffectDropCause>>,
    ) -> Result<WrittenEffects, ZeroHourError> {
        let (game, art) = (self.game, self.art);
        let LookSources {
            combat,
            types,
            start,
            additions,
        } = sources;
        // The systems the projectiles of the weapons the import converts carry.
        let mut trails = combat.exhausts(&game.weapons);
        // And the systems the treads throw, which a look names as it names a trail
        // (02-engine-core.md, A tread throws dust while it drives).
        let treads = game.objects.objects().flat_map(IniObject::draws);
        trails.extend(treads.flat_map(|draw| draw.tread_dust.iter().flatten().map(String::as_str)));
        // Each sound a weapon, an effect or a look names, its files read once and held until the
        // package is written (09-zero-hour.md, A sound is PCM, the music the game's MP3).
        let install = self.install;
        let mut sounds = SoundImport::new(&game.audio, install.language(), |path| {
            if install.contains(path) {
                install.read(path).map(Some)
            } else {
                Ok(None)
            }
        });
        let effect_sources = EffectSources {
            systems: &game.particles,
            lists: &game.effects,
            trails: &trails,
            cap: game.game_data.max_particles,
            texture: |name: &str| art.texture(name),
        };
        let mut effects = EffectsImport::of(effect_sources, &mut sounds)?;
        effects.file.set_wrecks(Self::wreck_ground(game))?;
        effects
            .file
            .set_treads(Self::tread_dust(), Self::track_marks(game))?;
        effects.name_deaths(&game.objects, |name: &str| art.texture(name), looks)?;
        self.losses
            .extend(effects.dropped.iter().cloned().map(ImportLoss::Effect));
        self.losses
            .extend(model_losses.into_iter().map(ImportLoss::Effect));
        combat.complete_looks(additions, &mut looks.units);
        let converted = combat.converted_shots();
        let shots_of = |weapon: &str| converted.get(weapon).map_or(&[][..], Vec::as_slice);
        let actions =
            effects.action_looks(&game.weapons, shots_of, &mut sounds, &mut looks.units)?;
        // A look of no unit type is drawn by nothing, and its models, animations and textures go
        // with it, unless another look draws them (09-zero-hour.md, A look is a unit type's).
        let looked = looks.units.len();
        looks.units.retain(|name, _| types.holds(name));
        let unnamed = looked - looks.units.len();
        if unnamed > 0 {
            self.losses
                .push(ImportLoss::UnnamedLooks { looks: unnamed });
        }
        // Each look's voices, and who sees it on the minimap, and the music of each faction's base
        // side (02-engine-core.md, A look names its voices; 02-engine-core.md, The minimap is drawn
        // from the terrain at load; 02-engine-core.md, Music by a choice).
        for object in game.objects.objects() {
            let name = TypeName::of(object)?;
            if let Some(look) = looks.units.get_mut(name.declared()) {
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
        effects.file.add_sounds(sounded.events)?;
        self.writer
            .write(EffectsFile::path(), &effects.file.encode())?;
        self.writer.write(SoundFile::path(), &sound_file.encode())?;
        for (to, bytes) in sounded.files {
            self.writer.write(to, &bytes)?;
        }
        self.losses
            .extend(sounded.dropped.into_iter().map(ImportLoss::Sound));
        Ok(WrittenEffects {
            file: effects.file,
            textures: effects.textures,
            actions,
        })
    }

    /// The ground the wrecks fall to, by the game's gravity (02-engine-core.md, Wrecks move by the
    /// client's step): an object's friction on the ground, `DEFAULT_FORWARD_FRICTION` and
    /// `DEFAULT_LATERAL_FRICTION`, takes 0.15 of its speed a frame, and it rests below
    /// `STUN_RELIEF_EPSILON`, half a world unit a frame.
    fn wreck_ground(game: &ZeroHourRules) -> WreckGround {
        WreckGround {
            gravity: game.game_data.gravity,
            stiffness: game.game_data.ground_stiffness,
            friction: 0.15,
            rest: 0.5 * game_frames::PER_SECOND,
        }
    }

    /// The dust a tread throws: it runs while its unit moves faster than the game's
    /// `DEBRIS_THRESHOLD`, a squared speed of 0.00001 a frame, scaled across by half its speed a
    /// frame and a tenth, and up and in count by its speed a frame and a tenth, each up to 1
    /// (`W3DTankDraw::doDrawModule`).
    fn tread_dust() -> TreadDust {
        let per_frame = |scale: f32| scale / game_frames::PER_SECOND;
        TreadDust {
            moving: 0.00001_f32.sqrt() * game_frames::PER_SECOND,
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
        }
    }

    /// The tracks a tread lays, when the game makes track marks: an edge each cell of 10 it moves,
    /// 0.2 of a cell above the ground, up to `GlobalData`'s 100 edges, of which the newest 25 stay
    /// opaque, each fading over 300 s (`W3DModelDraw`, `W3DTerrainTracks.cpp`).
    fn track_marks(game: &ZeroHourRules) -> TrackMarks {
        if game.game_data.track_marks {
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
        }
    }

    /// Writes the models and the animations of `converted` that `looks` and `effects` name,
    /// each once, the rest dropped unwritten; the textures of those models, and those a model
    /// names that the install lacks join the losses.
    fn write_models(
        &mut self,
        looks: &ClientUnits,
        effects: &EffectsFile,
        mut converted: ConvertedFiles,
    ) -> Result<BTreeSet<ArchivePath>, ZeroHourError> {
        let mut textures = BTreeSet::new();
        let mut missing = BTreeSet::new();
        let looks = TomlValue::of(looks);
        let effects = TomlValue::of(effects);
        let named = looks.strings().into_iter().chain(effects.strings());
        for named in named.filter_map(PackagePath::parse) {
            if let Some(model) = converted.models.remove(&named) {
                self.writer.write(named, &model.glb)?;
                textures.extend(model.textures);
                missing.extend(model.missing_textures);
            } else if let Some(animation) = converted.animations.remove(&named) {
                self.writer.write(named, &animation)?;
            }
        }
        self.losses
            .extend(missing.into_iter().map(ImportLoss::Texture));
        Ok(textures)
    }

    /// Writes each texture of `files` at its path, converted to KTX2: the textures the models,
    /// the terrain and the effects draw with, no other file of the install. A terrain's texture,
    /// read for its size into `read`, is not read again.
    fn write_textures(
        &mut self,
        files: BTreeMap<PackagePath, ArchivePath>,
        mut read: BTreeMap<ArchivePath, TerrainTexture>,
    ) -> Result<(), ZeroHourError> {
        for (to, path) in files {
            let kind = path
                .extension()
                .and_then(TextureKind::of)
                .expect("a drawn texture is DDS or TGA");
            let bytes = match read.remove(&path) {
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
            self.writer.write(to, &ktx2)?;
        }
        Ok(())
    }

    /// Writes the mode's data of `mode`, the units' file `units`, the mode's text of `texts`, and
    /// its manifest, which caps no speed past the fastest type's.
    fn write_mode(
        &mut self,
        mode: ModeImport<'_>,
        units: &WrittenUnits,
        texts: &BTreeMap<String, String>,
    ) -> Result<(), ZeroHourError> {
        self.writer
            .write(ModeFile::path(), mode.data().as_bytes())?;
        self.writer
            .write(UnitsFile::path(), units.text.as_bytes())?;
        // A mode whose types drive none caps no speed it moves at; 1 m a second is as good as any.
        let max_move_speed = units
            .max_speed
            .and_then(Speed::new)
            .unwrap_or(Speed::new(Num::ONE).expect("1 m a second is a speed"));
        let manifest = rules_package::mode_manifest(max_move_speed);
        let text = rules_package::mode_text(texts);
        self.writer
            .write(manifest.header.language.text_path(), text.as_bytes())?;
        let manifest = Toml::write(&manifest);
        self.writer
            .write(PackageFiles::manifest_path(), manifest.as_bytes())?;
        Ok(())
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
        read: &mut BTreeMap<ArchivePath, TerrainTexture>,
    ) -> Result<Option<PackageTexture>, ZeroHourError> {
        let Some(path) = self.art.texture(name) else {
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
            let size = match path.extension().and_then(TextureKind::of) {
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

    /// Where each texture of `drawn` goes in the package, each once; an error for two that go to
    /// one file, as an `x.dds` and an `x.tga` of one directory would.
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
}

#[cfg(test)]
mod tests;
