use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_actions::ActionKind;
use campfire_client_files::{ClientObjects, ClientUnits, MapConditions, TerrainFile};
use campfire_codec::Binary;
use campfire_combat::ModeCombatData;
use campfire_common::{DeclaredName, MapName};
use campfire_import::{
    ButtonDropCause, Dropped, ImportLoss, Imported, SupplyDropCause, WeaponDropCause,
};
use campfire_math::Num;
use campfire_navigation::MapNavigationData;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_production::{DropOffData, NodeData};
use tracing::info;

use crate::attack_play::AttackPlay;
use crate::base_play::BasePlay;
use crate::defense_play::DefensePlay;
use crate::error::CheckError;
use crate::failure::{CheckFailure, Differs};
use crate::golden::{Golden, Measured};
use crate::missile_play::MissilePlay;
use crate::pins::{
    FactorySteps, FightSteps, Fingerprints, FoughtCounts, HZ, LookCounts, MapCounts,
    ProducedCounts, ProductionSteps, SkirmishTicks, SuppliedCounts, SupplySteps, TerrainCounts,
    TerrainDrawn,
};
use crate::skirmish_play::{SkirmishPlay, SkirmishSteps};
use crate::start_drive::StartDrive;
use crate::supply_play::SupplyPlay;

/// The checks of an import, a group at a time, each failure it finds kept: the import's report,
/// its game package, the mode of `MAP` loaded from it, the values the golden file pins, none in
/// a run that blesses, and what the checks measured of them.
#[derive(Debug)]
pub(crate) struct ImportCheck<'a> {
    pub(crate) imported: &'a Imported,
    pub(crate) game: &'a PackageDir,
    pub(crate) map: &'a MapName,
    pub(crate) packages: ModePackages,
    pub(crate) golden: Option<Golden>,
    pub(crate) measured: Measured,
    pub(crate) failures: Vec<CheckFailure>,
}

impl ImportCheck<'_> {
    /// The value the golden file pins of `got`, which `of` reads; `got` itself in a run that
    /// blesses, which pins what it measures.
    fn pinned<T>(&self, of: impl FnOnce(&Golden) -> T, got: T) -> T {
        self.golden.as_ref().map_or(got, of)
    }

    /// The import's fingerprints, `got`.
    pub(crate) fn fingerprints(&mut self, got: Fingerprints) {
        let expected = self.pinned(|golden| golden.fingerprints, got);
        self.failures
            .extend(Differs::of(got, expected).map(CheckFailure::Fingerprints));
        self.measured.fingerprints = Some(got);
    }

    /// Tournament Desert's heights, units, client objects, markers, terrain and walls.
    pub(crate) fn map(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let heights = self
            .packages
            .heights()
            .map_or([0, 0], |grid| [grid.columns(), grid.rows()]);
        let placed = &self.packages.map().units;
        let by_type = tally(placed.iter().map(|unit| unit.unit_type.as_str()));
        let client: ClientObjects = files.read_data(&ClientObjects::path(self.map))?;
        let by_look = tally(client.objects.iter().map(|object| object.look.as_str()));
        info!(units = %listed(&by_type), "Tournament Desert's units by type");
        info!(objects = %listed(&by_look), "Tournament Desert's client objects by look");
        let navigation = self.packages.map().section::<MapNavigationData>();
        let counts = MapCounts {
            heights,
            units: placed.len(),
            unit_types: by_type.len(),
            client_objects: client.objects.len(),
            object_looks: by_look.len(),
            markers: self.packages.map().markers.len(),
            walls: navigation.map_or(0, |navigation| navigation.walls.len()),
        };
        let terrain = files.read_file(&TerrainFile::path(self.map))?;
        let terrain = Binary::decode::<TerrainFile>(&terrain).map_err(CheckError::Decode)?;
        let drawn = TerrainDrawn {
            quads: terrain.quads().ground.len(),
            counts: TerrainCounts {
                overlays: terrain.quads().overlays.len(),
                copies: terrain.texture().copies.len(),
                wraps: terrain.texture().wraps.len(),
            },
        };
        let untextured = self.imported.losses.iter().find_map(|loss| match loss {
            ImportLoss::UntexturedCells { map, cells } if map == self.map => Some(*cells),
            _ => None,
        });
        let untextured = untextured.unwrap_or(0);
        if untextured > 0 {
            self.failures.push(CheckFailure::Untextured(untextured));
        }
        let pinned = self.pinned(|golden| golden.map, counts);
        self.failures
            .extend(Differs::of(counts, pinned).map(CheckFailure::Map));
        let layers = self.pinned(|golden| golden.terrain, drawn.counts);
        let expected = TerrainDrawn::expected(pinned.heights, layers);
        self.failures
            .extend(Differs::of(drawn, expected).map(CheckFailure::Terrain));
        self.measured.map = Some(counts);
        self.measured.terrain = Some(drawn.counts);
        info!(
            samples = %format!("{} × {}", heights[0], heights[1]),
            units = counts.units,
            types = counts.unit_types,
            objects = counts.client_objects,
            looks = counts.object_looks,
            markers = counts.markers,
            quads = drawn.quads,
            overlays = drawn.counts.overlays,
            copies = drawn.counts.copies,
            wraps = drawn.counts.wraps,
            untextured,
            walls = counts.walls,
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
        for other in &self.imported.maps {
            let held: MapConditions = files.read_data(&MapConditions::path(other))?;
            drawn.night += usize::from(held.conditions.contains(&night));
            drawn.snowy += usize::from(held.conditions.contains(&snow));
        }
        let expected = self.pinned(|golden| golden.looks, drawn);
        self.failures
            .extend(Differs::of(drawn, expected).map(CheckFailure::Looks));
        self.measured.looks = Some(drawn);
        info!(
            varied = drawn.varied,
            empty = drawn.empty,
            night = drawn.night,
            snowy = drawn.snowy,
            "the looks of night and snow"
        );
        Ok(())
    }

    /// The waypoints the import dropped of every map it wrote, which loads in the imported
    /// mode, as the import's own load checks.
    pub(crate) fn other_maps(&self) {
        for loss in &self.imported.losses {
            if let ImportLoss::Waypoint { map, waypoint } = loss {
                info!(%map, waypoint, "a map's waypoint past its bounds is dropped");
            }
        }
        info!(maps = self.imported.maps.len(), "the imported maps loaded");
    }

    /// The trains and builds of every command set, and the buttons dropped, by cause.
    pub(crate) fn production(&mut self) {
        let dropped: Vec<&Dropped<ButtonDropCause>> = self
            .imported
            .losses
            .iter()
            .filter_map(|loss| match loss {
                ImportLoss::Button(button) => Some(button),
                _ => None,
            })
            .collect();
        let produced = ProducedCounts {
            trains: self.kind_count(ActionKind::Train),
            builds: self.kind_count(ActionKind::Build),
            dropped: ButtonDropCause::ALL.map(|cause| {
                dropped
                    .iter()
                    .filter(|button| button.cause == cause)
                    .count()
            }),
        };
        let expected = self.pinned(|golden| golden.produced, produced);
        self.failures
            .extend(Differs::of(produced, expected).map(CheckFailure::Produced));
        self.measured.produced = Some(produced);
        for dropped in &dropped {
            info!(
                button = dropped.name,
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

    /// The docks' nodes, the supply centers' drop-offs, the gatherers' gathers, the hackers'
    /// hacks, each action whose channel pays, and the supply dropped, by cause.
    pub(crate) fn supply(&mut self) {
        let dropped: Vec<&Dropped<SupplyDropCause>> = self
            .imported
            .losses
            .iter()
            .filter_map(|loss| match loss {
                ImportLoss::Supply(supply) => Some(supply),
                _ => None,
            })
            .collect();
        let units = self.packages.content().units.values();
        let (nodes, drop_offs) = units.fold((0, 0), |(nodes, drop_offs), unit| {
            (
                nodes + usize::from(unit.section::<NodeData>().is_some()),
                drop_offs + usize::from(unit.section::<DropOffData>().is_some()),
            )
        });
        let supplied = SuppliedCounts {
            nodes,
            drop_offs,
            gathers: self.kind_count(ActionKind::Gather),
            hacks: self
                .packages
                .content()
                .actions()
                .values()
                .filter(|action| {
                    let channel = action.keys.channel.as_ref();
                    channel.is_some_and(|channel| channel.pays.is_some())
                })
                .count(),
            dropped: SupplyDropCause::ALL.map(|cause| {
                dropped
                    .iter()
                    .filter(|supply| supply.cause == cause)
                    .count()
            }),
        };
        let expected = self.pinned(|golden| golden.supplied, supplied);
        self.failures
            .extend(Differs::of(supplied, expected).map(CheckFailure::Supplied));
        self.measured.supplied = Some(supplied);
        for dropped in &dropped {
            info!(
                object = dropped.name,
                cause = ?dropped.cause,
                "an object's supply is not imported"
            );
        }
        info!(
            nodes = supplied.nodes,
            drop_offs = supplied.drop_offs,
            gathers = supplied.gathers,
            hacks = supplied.hacks,
            dropped = ?supplied.dropped,
            "the docks, supply centers, gatherers and hackers"
        );
    }

    /// The weapons, the armors, the damage kinds and the bodies of no life or a wounded start.
    pub(crate) fn combat(&mut self) {
        let combat = self.packages.data().section_or_default::<ModeCombatData>();
        let losses = &self.imported.losses;
        let dropped: Vec<&Dropped<WeaponDropCause>> = losses
            .iter()
            .filter_map(|loss| match loss {
                ImportLoss::Weapon(weapon) => Some(weapon),
                _ => None,
            })
            .collect();
        let count = |kind: fn(&ImportLoss) -> bool| losses.iter().filter(|loss| kind(loss)).count();
        let fought = FoughtCounts {
            weapons: self.kind_count(ActionKind::Attack),
            dropped: WeaponDropCause::ALL.map(|cause| {
                dropped
                    .iter()
                    .filter(|weapon| weapon.cause == cause)
                    .count()
            }),
            armors: combat.armors.len(),
            kinds: combat.damage_kinds.len(),
            unliving: count(|loss| matches!(loss, ImportLoss::Unliving(_))),
            wounded: count(|loss| matches!(loss, ImportLoss::Wounded(_))),
        };
        let expected = self.pinned(|golden| golden.fought, fought);
        self.failures
            .extend(Differs::of(fought, expected).map(CheckFailure::Fought));
        self.measured.fought = Some(fought);
        for dropped in &dropped {
            info!(
                weapon = dropped.name,
                cause = ?dropped.cause,
                "a weapon is not imported"
            );
        }
        info!(%fought, "the weapons, armors and bodies");
    }

    /// The skirmish's production and fight, in a match of its own of the packages loaded again,
    /// then the start and the dozer's drive, at `HZ` ticks a second; then, in a run that blesses,
    /// the golden file written of what the checks measured: every failure the checks found.
    pub(crate) fn plays(mut self) -> Result<Vec<CheckFailure>, CheckError> {
        let rate = u32::try_from(HZ).ok().and_then(NonZeroU32::new);
        let rate = rate.expect("a rate");
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(SkirmishSteps {
            production,
            factory,
            fight,
            bot,
        }) = SkirmishPlay::run(played, rate, &mut self.failures)
        {
            let ticks = SkirmishTicks {
                barracks: production.barracks_ticks,
                factory: factory.ticks,
                shot: fight.shot_ticks,
                hit: fight.hit_ticks,
                fight: fight.fight_ticks,
            };
            let pinned = self.pinned(|golden| golden.skirmish, ticks);
            let expected = ProductionSteps::expected(pinned.barracks);
            self.failures
                .extend(Differs::of(production, expected).map(CheckFailure::Production));
            info!(
                dozer = production.dozer_ticks,
                barracks = production.barracks_ticks,
                ranger = production.ranger_ticks,
                money = ?production.money,
                "player 1 trained a dozer, built a barracks and trained a ranger"
            );
            let expected = FactorySteps::expected(pinned.factory);
            self.failures
                .extend(Differs::of(factory, expected).map(CheckFailure::Factory));
            info!(
                ticks = ?factory.ticks,
                money = factory.money,
                "player 1 built a power plant, a supply center and a war factory"
            );
            let expected = FightSteps::expected(pinned);
            self.failures
                .extend(Differs::of(fight, expected).map(CheckFailure::Fight));
            self.measured.skirmish = Some(ticks);
            info!(
                train = fight.train_ticks,
                shot = ?fight.shot_ticks,
                hit = ?fight.hit_ticks,
                fight = fight.fight_ticks,
                money = fight.money,
                result = ?fight.result,
                "player 1's crusaders' shells destroyed the bot's command center"
            );
            let expected = self.pinned(|golden| golden.bot, bot);
            self.failures
                .extend(Differs::of(bot, expected).map(CheckFailure::Bot));
            self.measured.bot = Some(bot);
            info!(
                built = ?bot.built,
                farthest = bot.farthest_m,
                "the builder bot built its base and trained its units"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(supply) = SupplyPlay::run(played, rate, &mut self.failures) {
            let ticks = supply.ticks();
            let expected = SupplySteps::expected(self.pinned(|golden| golden.supply, ticks));
            self.failures
                .extend(Differs::of(supply, expected).map(CheckFailure::Supply));
            self.measured.supply = Some(ticks);
            info!(
                built = ?supply.built,
                money = supply.built_money,
                deliveries = ?supply.deliveries,
                "player 1 built a supply center as China, and its free truck delivered its loads"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(defense) = DefensePlay::GATTLING.run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.defense, defense);
            self.failures
                .extend(Differs::of(defense, expected).map(CheckFailure::Defense));
            self.measured.defense = Some(defense);
            info!(
                built = ?defense.built,
                hit = defense.hit,
                death = defense.death,
                "player 1's Gattling Cannon, ordered nothing, spun up and killed the bot's dozer"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(patriot) = DefensePlay::PATRIOT.run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.patriot, patriot);
            self.failures
                .extend(Differs::of(patriot, expected).map(CheckFailure::Patriot));
            self.measured.patriot = Some(patriot);
            info!(
                built = ?patriot.built,
                hit = patriot.hit,
                death = patriot.death,
                "player 1's Patriot battery, ordered nothing, killed the bot's dozer"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(stinger) = DefensePlay::STINGER.run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.stinger, stinger);
            self.failures
                .extend(Differs::of(stinger, expected).map(CheckFailure::Stinger));
            self.measured.stinger = Some(stinger);
            info!(
                built = ?stinger.built,
                hit = stinger.hit,
                death = stinger.death,
                "player 1's Stinger site's soldiers, ordered nothing, killed the bot's dozer"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(missile) = MissilePlay::run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.missile, missile);
            self.failures
                .extend(Differs::of(missile, expected).map(CheckFailure::Missile));
            self.measured.missile = Some(missile);
            info!(
                built = missile.built,
                trained = missile.trained,
                shot = missile.shot,
                hit = missile.hit,
                death = missile.death,
                "player 1's Tank Hunter, ordered nothing, killed the bot's dozer with its missiles"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(base) = BasePlay::run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.base, base);
            self.failures
                .extend(Differs::of(base, expected).map(CheckFailure::Base));
            self.measured.base = Some(base);
            info!(
                ended = ?base.ended,
                result = ?base.result,
                standing = ?base.standing,
                "two bots started with their bases, and fought"
            );
        }
        let played = ModePackages::at(self.game, Some(self.map)).map_err(CheckError::Load)?;
        if let Some(attack) = AttackPlay::run(played, rate, &mut self.failures) {
            let expected = self.pinned(|golden| golden.attack, attack);
            self.failures
                .extend(Differs::of(attack, expected).map(CheckFailure::Attack));
            self.measured.attack = Some(attack);
            info!(
                ended = ?attack.ended,
                result = ?attack.result,
                standing = ?attack.standing,
                "the attacker bot built its base and fought the builder bot"
            );
        }
        if let Some(drive) = StartDrive::run(self.packages, rate, &mut self.failures) {
            let golden = self.golden.as_ref();
            let expected = golden.map_or(drive.ticks, |golden| golden.drive_ticks);
            self.failures
                .extend(Differs::of(drive.ticks, expected).map(CheckFailure::DriveTicks));
            self.measured.drive_ticks = Some(drive.ticks);
            if drive.left != Num::ZERO {
                self.failures.push(CheckFailure::DriveShort(drive.left));
            }
            info!(
                ticks = drive.ticks,
                end = %format!("({}, {})", drive.end.x, drive.end.z),
                left = %drive.left,
                "the dozer drove from player 1's start to player 2's"
            );
        }
        if self.golden.is_none() {
            match self.measured.into_golden() {
                Some(golden) => golden.bless()?,
                None => self.failures.push(CheckFailure::Unmeasured),
            }
        }
        Ok(self.failures)
    }

    /// How many actions of `kind` the mode holds.
    fn kind_count(&self, kind: ActionKind) -> usize {
        let actions = self.packages.content().actions().values();
        actions.filter(|action| action.kind() == kind).count()
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
