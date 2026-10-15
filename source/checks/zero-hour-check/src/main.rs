//! The Zero Hour install check, on request: imports the install `GENERALSZH_DATA` names into the
//! directory its command line names, in place of the packages an earlier run wrote there once
//! the new ones are whole, and checks the fingerprints of the imported package and the rules
//! package against those it pins, which a run on Linux and a run on macOS gave. Then it checks
//! that Tournament Desert imports as a multiplayer game of the original spawns it: 270 × 340 height samples; of its 580
//! objects, once its 176 road ends are dropped, 142 placed units of 41 types and 438 client
//! objects of 22 looks, each kind's templates logged for review; and 28 markers. It checks its
//! terrain file's 269 × 339 ground quads, its overlays, and its texture's copies and border
//! wraps, each logged for review; that the import reports no cell of its terrain whose tile has
//! no texture; its 286 walls of its grown cliffs; and that it loads in a match of the imported
//! mode, where player 1 starts with America's command center at `Player_1_Start` and its dozer
//! clear of the center's box, and the dozer, ordered to `Player_2_Start`, gets there within
//! `DRIVE_TICKS`, in no wall's cell in any tick. It counts the trains and builds the import
//! converts from the command sets and the buttons it drops by cause; and in a match of its own,
//! player 1's command center trains a dozer, its dozer builds a barracks, and the barracks trains
//! a ranger, each in its pinned ticks and paid from the starting money. Every other map the game
//! lists loads in the imported mode. It fails
//! when `GENERALSZH_DATA` names no install, so a machine with no copy never passes as one that
//! checked.
//!
//! Run it with `./drive.py zero-hour [out]`, which builds it with the `release-dev` profile and
//! writes to `~/zh-package` unless `out` names another directory.

use std::collections::BTreeMap;
use std::env;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use campfire_actions::ActionKind;
use campfire_client_files::{ClientObjects, ClientUnits, HeightGrid, MapConditions, TerrainFile};
use campfire_codec::{Binary, Toml, TomlError};
use campfire_common::{ExitStatus, MapName, PackageFingerprint};
use campfire_import::{DropCause, DroppedButton, GameVersion, WeaponDrop, ZeroHour};
use campfire_log::{ErrorReport, Logging};
use campfire_math::Num;
use campfire_mode::MatchResult;
use campfire_package::{ContentError, ModePackages, PackageReader};
use campfire_package_disk::PackageDir;
use campfire_store::{DirEntries, DurableDir, EntryKind, ReadError};
use campfire_values::{DeclaredName, PackagePath, Team};
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::CheckError;
use crate::failure::{
    Failure, FightSteps, Fingerprints, FoughtCounts, LookCounts, ProducedCounts, ProductionSteps,
    TerrainDrawn,
};
use crate::skirmish_play::{SkirmishPlay, SkirmishSteps};
use crate::start_drive::StartDrive;

mod command_line;
mod error;
mod failure;
mod skirmish_play;
mod start_drive;

/// The map the check plays.
const MAP: &str = "tournament_desert";

/// The fingerprints of the imported package and of the rules package that the known version and
/// this importer give on every OS: those a run on Linux and a run on macOS gave (design 12, D4,
/// D7). An importer that converts anything otherwise moves them, and a run on each OS pins them
/// again.
const FINGERPRINTS: Fingerprints = Fingerprints {
    game: PackageFingerprint::of_hex(
        "f5c57357cb7ec525f8b2bdbe62620f4e24792303956bd38e224541565e4c376b",
    ),
    rules: PackageFingerprint::of_hex(
        "d0245c9b5ddd9cb2cb58aeb2e69e92f3779b30cbbca0aa291a47ef35cd3aa874",
    ),
};

/// Tournament Desert's walls: its 48,689 cliff cells grown by one ring, merged row by row
/// (design 17, D3).
const WALLS: usize = 286;

/// The most ticks player 1's dozer takes from its start to player 2's on Tournament Desert, at 30
/// ticks a second, which the check pins: what it took, 57.2 s.
const DRIVE_TICKS: u64 = 1717;

/// The looks the import gives night or snow variants, of unit types and of client objects; the
/// unit types' looks of no models, of objects that draw nothing; and the maps of night and of
/// snow among the 150.
const LOOKS: LookCounts = LookCounts {
    varied: 382,
    empty: 273,
    night: 15,
    snowy: 17,
};

/// The trains and builds the import converts from every command set, and the buttons it drops,
/// by cause in the order of `DropCause::ALL`: no object, a science 22, an either-or 4, no walker 36,
/// as the aircraft an airfield or a supply center trains, no box 24, no queue, no type, a bad
/// time (design 18, D3).
const PRODUCED: ProducedCounts = ProducedCounts {
    trains: 187,
    builds: 138,
    dropped: [0, 22, 4, 36, 24, 0, 0, 0],
};

/// The production play's ticks and money on Tournament Desert: the dozer's 5 s, 150 ticks, the
/// order's tick among them; the barracks' 706, the dozer's drive round the command center's box
/// to the site, which the check pins as it measured it, then its 10 s of building, 300 ticks;
/// the ranger's 5 s; and the money of 10,000 less the dozer's 1,000, the barracks' 600 and the
/// ranger's 225.
const PRODUCTION: ProductionSteps = ProductionSteps {
    dozer_ticks: 150,
    barracks_ticks: 706,
    ranger_ticks: 150,
    money: [9000, 8400, 8175],
};

/// The fight on Tournament Desert against an idle bot: the barracks' 19 more rangers, the first
/// of the production steps trained, each its 150 ticks one after another, 2,850; their walk to
/// the bot's command center and its destruction, 5,000 life at a ranger's 5 small arms halved,
/// which the check pins as it measured it; the money of 8,175 less 19 rangers' 225 each; player
/// 1's team the winner; and the bot's dozer's life gone.
const FIGHT: FightSteps = FightSteps {
    train_ticks: 19 * 150,
    fight_ticks: 3199,
    money: 8175 - 19 * 225,
    result: MatchResult::Won(Team::new(0)),
    killed: true,
};

/// The weapons the import converts from every starting weapon set, the weapons it drops by cause
/// in the order of `WeaponDrop::ALL`: no weapon, a projectile 70, a radius 6, a scatter, a speed
/// that may delay the damage 7, no hit on the ground 7, a suicide, damage at its own place, a
/// fire that speeds up 3, a delay before the attack 4, a minimum range, a random delay, no delay
/// 3, no damage type, a bad number; its armors, `Armor.ini`'s 53, and damage kinds, the game's
/// 38; and the unit types' objects of no life, 38, and of a wounded start (design 19, D5, D6).
const FOUGHT: FoughtCounts = FoughtCounts {
    weapons: 21,
    dropped: [0, 70, 6, 0, 7, 7, 0, 0, 3, 4, 0, 0, 3, 0, 0],
    armors: 53,
    kinds: 38,
    unliving: 38,
    wounded: 0,
};

/// Tournament Desert's terrain: a ground quad for each of the 269 × 339 cells the game draws;
/// its overlays; a copy for each tile its classes' textures load; and a border wrap for each
/// class's block.
const TERRAIN: TerrainDrawn = TerrainDrawn {
    quads: 269 * 339,
    overlays: 19_041,
    copies: 300,
    wraps: 19,
};

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let out = match Logging::command_line::<CommandLine>(env::args_os()) {
        Ok(line) => line.out,
        Err(status) => return ExitCode::from(status),
    };
    match run(&out) {
        Ok(failures) if failures.is_empty() => {
            info!("the Zero Hour check passed");
            ExitCode::from(ExitStatus::Success)
        }
        Ok(failures) => {
            for failure in &failures {
                error!(%failure, "a failure of the Zero Hour check");
            }
            error!(failures = failures.len(), "the Zero Hour check failed");
            ExitCode::from(ExitStatus::Failure)
        }
        Err(problem) => {
            error!(error = %ErrorReport::of(&problem), "the Zero Hour check did not run");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}

/// Imports the install into `out`, in place of the packages an earlier run wrote there once the
/// new ones are whole, and checks Tournament Desert; the failures it found.
fn run(out: &Path) -> Result<Vec<Failure>, CheckError> {
    let install = env::var_os("GENERALSZH_DATA")
        .filter(|install| !install.is_empty())
        .map(PathBuf::from)
        .ok_or(CheckError::NoInstall)?;
    let zero_hour = ZeroHour::open(&install, GameVersion::KNOWN).map_err(CheckError::Import)?;
    let name = out
        .file_name()
        .ok_or_else(|| CheckError::NoName(out.to_owned()))?;
    let mut staged = name.to_owned();
    staged.push(".new");
    let stage = out.with_file_name(staged);
    clear(&stage)?;
    let imported = zero_hour.write(&stage).map_err(CheckError::Import)?;
    clear(out)?;
    DurableDir::rename(&stage, out).map_err(CheckError::Rename)?;
    info!(
        version = zero_hour.version().name,
        fingerprint = %imported.fingerprint,
        rules = %imported.rules,
        packages = %out.display(),
        "imported"
    );
    let mut failures = Vec::new();
    let given = Fingerprints {
        game: imported.fingerprint,
        rules: imported.rules,
    };
    if given != FINGERPRINTS {
        failures.push(Failure::Fingerprints(given));
    }
    let map = MapName::new(MAP).expect("a map's name");
    let packages = PackageDir::new(out.join(ZeroHour::GAME))
        .mode_packages(&map)
        .map_err(CheckError::Load)?;
    let files = &packages.mode().package.files;
    let heights = read(files, &format!("map/{MAP}/heights.bin"))?;
    let grid = Binary::decode::<HeightGrid>(&heights).map_err(CheckError::Decode)?;
    if (grid.columns(), grid.rows()) != (270, 340) {
        failures.push(Failure::Heights {
            columns: grid.columns(),
            rows: grid.rows(),
        });
    }
    let placed = &packages.map().units;
    let units = placed.len();
    let by_type = tally(placed.iter().map(|unit| unit.unit_type.as_str()));
    let types = by_type.len();
    if (units, types) != (142, 41) {
        failures.push(Failure::Units { units, types });
    }
    let client: ClientObjects = read_toml(files, &ClientObjects::path(MAP), Toml::parse)?;
    let objects = client.objects.len();
    let by_look = tally(client.objects.iter().map(|object| object.look.as_str()));
    let looks = by_look.len();
    if (objects, looks) != (438, 22) {
        failures.push(Failure::ClientObjects { objects, looks });
    }
    info!(units = %listed(&by_type), "Tournament Desert's units by type");
    info!(objects = %listed(&by_look), "Tournament Desert's client objects by look");
    let markers = packages.map().markers.len();
    if markers != 28 {
        failures.push(Failure::Markers(markers));
    }
    // The looks that draw other models on a night or snowy map, those that draw nothing, and the
    // maps of night and of snow.
    let unit_looks: ClientUnits = read_toml(files, "client/units.toml", Toml::parse)?;
    let varied = unit_looks
        .units
        .values()
        .chain(unit_looks.objects.values())
        .filter(|look| !look.variants.is_empty())
        .count();
    let empty = unit_looks
        .units
        .values()
        .filter(|look| look.models.is_empty() && look.variants.is_empty())
        .count();
    let [night, snow] = ["night", "snow"].map(|name| DeclaredName::new(name).expect("a name"));
    let mut drawn = LookCounts {
        varied,
        empty,
        night: 0,
        snowy: 0,
    };
    for other in imported.untextured_cells.keys() {
        let held: MapConditions =
            read_toml(files, &MapConditions::path(other.as_str()), Toml::parse)?;
        drawn.night += usize::from(held.conditions.contains(&night));
        drawn.snowy += usize::from(held.conditions.contains(&snow));
    }
    if drawn != LOOKS {
        failures.push(Failure::Looks(drawn));
    }
    info!(
        varied = drawn.varied,
        empty = drawn.empty,
        night = drawn.night,
        snowy = drawn.snowy,
        "the looks of night and snow"
    );
    let terrain = read(files, &TerrainFile::path(MAP))?;
    let terrain = Binary::decode::<TerrainFile>(&terrain).map_err(CheckError::Decode)?;
    let drawn = TerrainDrawn {
        quads: terrain.quads().ground.len(),
        overlays: terrain.quads().overlays.len(),
        copies: terrain.texture().copies.len(),
        wraps: terrain.texture().wraps.len(),
    };
    if drawn != TERRAIN {
        failures.push(Failure::Terrain(drawn));
    }
    let untextured = imported.untextured_cells[&map];
    if untextured > 0 {
        failures.push(Failure::Untextured(untextured));
    }
    let navigation = packages.map().navigation.as_ref();
    let walls = navigation.map_or(0, |navigation| navigation.walls.len());
    if walls != WALLS {
        failures.push(Failure::Walls(walls));
    }
    info!(
        samples = %format!("{} × {}", grid.columns(), grid.rows()),
        units,
        types,
        objects,
        looks,
        markers,
        quads = drawn.quads,
        overlays = drawn.overlays,
        copies = drawn.copies,
        wraps = drawn.wraps,
        untextured,
        walls,
        "Tournament Desert loaded"
    );
    // Every other map the game lists loads in the imported mode, its walls and its placed bodies
    // checked as a match loads them.
    let game = PackageDir::new(out.join(ZeroHour::GAME));
    let mut loaded = 1;
    for other in imported
        .untextured_cells
        .keys()
        .filter(|&other| *other != map)
    {
        match game.mode_packages(other) {
            Ok(_) => loaded += 1,
            Err(error) => failures.push(Failure::MapLoad {
                map: other.clone(),
                error,
            }),
        }
    }
    info!(
        loaded,
        maps = imported.untextured_cells.len(),
        "the imported maps loaded"
    );
    for (map, waypoints) in &imported.dropped_waypoints {
        info!(
            %map,
            waypoints = %waypoints.join(", "),
            "a map's waypoints past its bounds are dropped"
        );
    }
    // The trains and builds of every command set, and the buttons dropped, by cause.
    let produced = ProducedCounts {
        trains: kind_count(&packages, ActionKind::Train),
        builds: kind_count(&packages, ActionKind::Build),
        dropped: dropped_by_cause(&imported.dropped_buttons),
    };
    if produced != PRODUCED {
        failures.push(Failure::Produced(produced));
    }
    for dropped in &imported.dropped_buttons {
        info!(
            button = dropped.button,
            cause = ?dropped.cause,
            "a command button is not imported"
        );
    }
    info!(
        trains = produced.trains,
        builds = produced.builds,
        dropped = ?produced.dropped,
        "the command sets' trains and builds"
    );
    // The weapons, the armors, the damage kinds and the bodies of no life or a wounded start.
    let combat = &packages.data().combat;
    let fought = FoughtCounts {
        weapons: kind_count(&packages, ActionKind::Attack),
        dropped: WeaponDrop::ALL.map(|cause| {
            let dropped = imported.dropped_weapons.iter();
            dropped.filter(|weapon| weapon.cause == cause).count()
        }),
        armors: combat.armors.len(),
        kinds: combat.damage_kinds.len(),
        unliving: imported.unliving.len(),
        wounded: imported.wounded.len(),
    };
    if fought != FOUGHT {
        failures.push(Failure::Fought(Box::new(fought)));
    }
    for dropped in &imported.dropped_weapons {
        info!(
            weapon = dropped.weapon,
            cause = ?dropped.cause,
            "a weapon is not imported"
        );
    }
    info!(%fought, "the weapons, armors and bodies");
    let rate = NonZeroU32::new(30).expect("a rate");
    // The play takes a match of its own, of the packages loaded again.
    let played = PackageDir::new(out.join(ZeroHour::GAME))
        .mode_packages(&map)
        .map_err(CheckError::Load)?;
    if let Some(SkirmishSteps { production, fight }) =
        SkirmishPlay::run(played, rate, &mut failures)
    {
        if production != PRODUCTION {
            failures.push(Failure::Production(production));
        }
        info!(
            dozer = production.dozer_ticks,
            barracks = production.barracks_ticks,
            ranger = production.ranger_ticks,
            money = ?production.money,
            "player 1 trained a dozer, built a barracks and trained a ranger"
        );
        if fight != FIGHT {
            failures.push(Failure::Fight(fight));
        }
        info!(
            train = fight.train_ticks,
            fight = fight.fight_ticks,
            money = fight.money,
            result = ?fight.result,
            killed = fight.killed,
            "player 1's rangers destroyed the bot's command center"
        );
    }
    if let Some(drive) = StartDrive::run(packages, rate, &mut failures) {
        if drive.ticks > DRIVE_TICKS {
            failures.push(Failure::DriveTicks(drive.ticks));
        }
        if drive.left != Num::ZERO {
            failures.push(Failure::DriveShort(drive.left));
        }
        info!(
            ticks = drive.ticks,
            end = %format!("({}, {})", drive.end.x, drive.end.z),
            left = %drive.left,
            "the dozer drove from player 1's start to player 2's"
        );
    }
    Ok(failures)
}

/// How many actions of `kind` the mode of `packages` holds.
fn kind_count(packages: &ModePackages, kind: ActionKind) -> usize {
    let actions = packages.content().actions.values();
    actions.filter(|action| action.kind == kind).count()
}

/// How many of `dropped` each cause drops, in the order of `DropCause::ALL`.
fn dropped_by_cause(dropped: &[DroppedButton]) -> [usize; DropCause::ALL.len()] {
    DropCause::ALL.map(|cause| {
        dropped
            .iter()
            .filter(|button| button.cause == cause)
            .count()
    })
}

/// Removes the packages an earlier run wrote to `out`, so the import writes it anew; nothing for
/// no directory there, and an error for one that holds anything but the import's two packages,
/// which no run removes.
fn clear(out: &Path) -> Result<(), CheckError> {
    let entries = match DirEntries::read(out) {
        Ok(entries) => entries,
        Err(error) if matches!(error.error, ReadError::Missing) => return Ok(()),
        Err(error) => return Err(CheckError::List(error)),
    };
    let packages = entries.iter().all(|entry| {
        entry.kind == EntryKind::Dir
            && [ZeroHour::GAME, ZeroHour::RULES].contains(&entry.name.to_str().unwrap_or_default())
    });
    if !packages {
        return Err(CheckError::NotPackages(out.to_owned()));
    }
    DurableDir::remove(out).map_err(CheckError::Remove)
}

/// How many of `names` there are of each name.
fn tally<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    counts
}

/// `counts` for a log: `name × count`, in the order of the names.
fn listed(counts: &BTreeMap<&str, usize>) -> String {
    let entries: Vec<String> = counts
        .iter()
        .map(|(name, count)| format!("{name} × {count}"))
        .collect();
    entries.join(", ")
}

/// The package's TOML file at `path`, as `parse` reads its text.
fn read_toml<T>(
    files: &PackageReader,
    path: &str,
    parse: fn(&str) -> Result<T, TomlError>,
) -> Result<T, CheckError> {
    let path = PackagePath::parse(path).expect("a map's file is a package path");
    let bytes = files.read_file(&path).map_err(CheckError::Read)?;
    let text = String::from_utf8(bytes).map_err(|error| {
        CheckError::Read(ContentError::NotText {
            path: path.clone(),
            error: error.utf8_error(),
        })
    })?;
    parse(&text).map_err(|error| CheckError::Read(ContentError::Data { path, error }))
}

/// The bytes of the package's file at `path`.
fn read(files: &PackageReader, path: &str) -> Result<Vec<u8>, CheckError> {
    let path = PackagePath::parse(path).expect("a map's file is a package path");
    files.read_file(&path).map_err(CheckError::Read)
}
