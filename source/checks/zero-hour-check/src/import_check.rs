use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_actions::ActionKind;
use campfire_client_files::{ClientObjects, ClientUnits, MapConditions, TerrainFile};
use campfire_codec::Binary;
use campfire_common::MapName;
use campfire_import::{DropCause, Imported, WeaponDrop};
use campfire_math::Num;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_values::DeclaredName;
use tracing::info;

use crate::error::CheckError;
use crate::failure::Failure;
use crate::pins::{
    BOT, CLIENT_OBJECTS, DRIVE_TICKS, FACTORY, FIGHT, FOUGHT, FoughtCounts, HEIGHTS, LOOKS,
    LookCounts, MARKERS, OBJECT_LOOKS, PRODUCED, PRODUCTION, ProducedCounts, TERRAIN, TerrainDrawn,
    UNIT_TYPES, UNITS, WALLS,
};
use crate::skirmish_play::{SkirmishPlay, SkirmishSteps};
use crate::start_drive::StartDrive;

/// The checks of an import against the pins, a group at a time, each failure it finds kept:
/// the import's report, its game package, and the mode of `MAP` loaded from it.
#[derive(Debug)]
pub(crate) struct ImportCheck<'a> {
    pub(crate) imported: &'a Imported,
    pub(crate) game: &'a PackageDir,
    pub(crate) map: &'a MapName,
    pub(crate) packages: ModePackages,
    pub(crate) failures: Vec<Failure>,
}

impl ImportCheck<'_> {
    /// Tournament Desert's heights, units, client objects, markers, terrain and walls.
    pub(crate) fn map(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let [columns, rows] = self
            .packages
            .heights()
            .map_or([0, 0], |grid| [grid.columns(), grid.rows()]);
        if [columns, rows] != HEIGHTS {
            self.failures.push(Failure::Heights { columns, rows });
        }
        let placed = &self.packages.map().units;
        let units = placed.len();
        let by_type = tally(placed.iter().map(|unit| unit.unit_type.as_str()));
        let types = by_type.len();
        if (units, types) != (UNITS, UNIT_TYPES) {
            self.failures.push(Failure::Units { units, types });
        }
        let client: ClientObjects = files.read_data(&ClientObjects::path(self.map))?;
        let objects = client.objects.len();
        let by_look = tally(client.objects.iter().map(|object| object.look.as_str()));
        let looks = by_look.len();
        if (objects, looks) != (CLIENT_OBJECTS, OBJECT_LOOKS) {
            self.failures
                .push(Failure::ClientObjects { objects, looks });
        }
        info!(units = %listed(&by_type), "Tournament Desert's units by type");
        info!(objects = %listed(&by_look), "Tournament Desert's client objects by look");
        let markers = self.packages.map().markers.len();
        if markers != MARKERS {
            self.failures.push(Failure::Markers(markers));
        }
        let terrain = files.read_file(&TerrainFile::path(self.map))?;
        let terrain = Binary::decode::<TerrainFile>(&terrain).map_err(CheckError::Decode)?;
        let drawn = TerrainDrawn {
            quads: terrain.quads().ground.len(),
            overlays: terrain.quads().overlays.len(),
            copies: terrain.texture().copies.len(),
            wraps: terrain.texture().wraps.len(),
        };
        if drawn != TERRAIN {
            self.failures.push(Failure::Terrain(drawn));
        }
        let untextured = self.imported.maps[self.map].untextured_cells;
        if untextured > 0 {
            self.failures.push(Failure::Untextured(untextured));
        }
        let navigation = self.packages.map().navigation.as_ref();
        let walls = navigation.map_or(0, |navigation| navigation.walls.len());
        if walls != WALLS {
            self.failures.push(Failure::Walls(walls));
        }
        info!(
            samples = %format!("{columns} × {rows}"),
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
        Ok(())
    }

    /// The looks that draw other models on a night or snowy map, those that draw nothing, and
    /// the maps of night and of snow among those the import wrote.
    pub(crate) fn looks(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let unit_looks: ClientUnits = files.read_data(&ClientUnits::path())?;
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
        for other in self.imported.maps.keys() {
            let held: MapConditions = files.read_data(&MapConditions::path(other))?;
            drawn.night += usize::from(held.conditions.contains(&night));
            drawn.snowy += usize::from(held.conditions.contains(&snow));
        }
        if drawn != LOOKS {
            self.failures.push(Failure::Looks(drawn));
        }
        info!(
            varied = drawn.varied,
            empty = drawn.empty,
            night = drawn.night,
            snowy = drawn.snowy,
            "the looks of night and snow"
        );
        Ok(())
    }

    /// Every other map the import wrote loads in the imported mode, its walls and its placed
    /// bodies checked as a match loads them.
    pub(crate) fn other_maps(&mut self) {
        let mut loaded = 1;
        for (other, imported) in &self.imported.maps {
            if other != self.map {
                match self.game.mode_packages(Some(other)) {
                    Ok(_) => loaded += 1,
                    Err(error) => self.failures.push(Failure::MapLoad {
                        map: other.clone(),
                        error,
                    }),
                }
            }
            if !imported.dropped_waypoints.is_empty() {
                info!(
                    map = %other,
                    waypoints = %imported.dropped_waypoints.join(", "),
                    "a map's waypoints past its bounds are dropped"
                );
            }
        }
        info!(
            loaded,
            maps = self.imported.maps.len(),
            "the imported maps loaded"
        );
    }

    /// The trains and builds of every command set, and the buttons dropped, by cause.
    pub(crate) fn production(&mut self) {
        let dropped = &self.imported.dropped_buttons;
        let produced = ProducedCounts {
            trains: self.kind_count(ActionKind::Train),
            builds: self.kind_count(ActionKind::Build),
            dropped: DropCause::ALL.map(|cause| {
                dropped
                    .iter()
                    .filter(|button| button.cause == cause)
                    .count()
            }),
        };
        if produced != PRODUCED {
            self.failures.push(Failure::Produced(produced));
        }
        for dropped in dropped {
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
    }

    /// The weapons, the armors, the damage kinds and the bodies of no life or a wounded start.
    pub(crate) fn combat(&mut self) {
        let combat = &self.packages.data().combat;
        let dropped = &self.imported.dropped_weapons;
        let fought = FoughtCounts {
            weapons: self.kind_count(ActionKind::Attack),
            dropped: WeaponDrop::ALL.map(|cause| {
                dropped
                    .iter()
                    .filter(|weapon| weapon.cause == cause)
                    .count()
            }),
            armors: combat.armors.len(),
            kinds: combat.damage_kinds.len(),
            unliving: self.imported.unliving.len(),
            wounded: self.imported.wounded.len(),
        };
        if fought != FOUGHT {
            self.failures.push(Failure::Fought(Box::new(fought)));
        }
        for dropped in dropped {
            info!(
                weapon = dropped.weapon,
                cause = ?dropped.cause,
                "a weapon is not imported"
            );
        }
        info!(%fought, "the weapons, armors and bodies");
    }

    /// The skirmish's production and fight, in a match of its own of the packages loaded again,
    /// then the start and the dozer's drive, at 30 ticks a second; every failure the checks found.
    pub(crate) fn plays(mut self) -> Result<Vec<Failure>, CheckError> {
        let rate = NonZeroU32::new(30).expect("a rate");
        let played = self
            .game
            .mode_packages(Some(self.map))
            .map_err(CheckError::Load)?;
        if let Some(SkirmishSteps {
            production,
            factory,
            fight,
            bot,
        }) = SkirmishPlay::run(played, rate, &mut self.failures)
        {
            if production != PRODUCTION {
                self.failures.push(Failure::Production(production));
            }
            info!(
                dozer = production.dozer_ticks,
                barracks = production.barracks_ticks,
                ranger = production.ranger_ticks,
                money = ?production.money,
                "player 1 trained a dozer, built a barracks and trained a ranger"
            );
            if factory != FACTORY {
                self.failures.push(Failure::Factory(factory));
            }
            info!(
                ticks = ?factory.ticks,
                money = factory.money,
                "player 1 built a power plant, a supply center and a war factory"
            );
            if fight != FIGHT {
                self.failures.push(Failure::Fight(fight));
            }
            info!(
                train = fight.train_ticks,
                shot = ?fight.shot_ticks,
                hit = ?fight.hit_ticks,
                fight = fight.fight_ticks,
                money = fight.money,
                result = ?fight.result,
                "player 1's crusaders' shells destroyed the bot's command center"
            );
            if bot != BOT {
                self.failures.push(Failure::Bot(bot));
            }
            info!(
                built = ?bot.built,
                farthest = bot.farthest_m,
                "the builder bot built its base and trained its units"
            );
        }
        if let Some(drive) = StartDrive::run(self.packages, rate, &mut self.failures) {
            if drive.ticks > DRIVE_TICKS {
                self.failures.push(Failure::DriveTicks(drive.ticks));
            }
            if drive.left != Num::ZERO {
                self.failures.push(Failure::DriveShort(drive.left));
            }
            info!(
                ticks = drive.ticks,
                end = %format!("({}, {})", drive.end.x, drive.end.z),
                left = %drive.left,
                "the dozer drove from player 1's start to player 2's"
            );
        }
        Ok(self.failures)
    }

    /// How many actions of `kind` the mode holds.
    fn kind_count(&self, kind: ActionKind) -> usize {
        let actions = self.packages.content().actions.values();
        actions.filter(|action| action.kind == kind).count()
    }
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
