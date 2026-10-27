use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use campfire_actions::ActionKind;
use campfire_client_files::{
    ActionLook, ClientFile, ClientObjects, ClientUnit, ClientUnits, EffectsFile, HudFile, LookDraw,
    MapConditions, SoundFile, TerrainFile,
};
use campfire_combat::ModeCombatData;
use campfire_common::{DeclaredName, MapName};
use campfire_import::{
    AnimationDropCause, ButtonDropCause, EffectDropCause, ImportLoss, Imported, PanelDropCause,
    RoadDropCause, SoundDropCause, StateDropCause, SupplyDropCause, UpgradeDropCause,
    WeaponDropCause,
};
use campfire_math::Num;
use campfire_navigation::MapNavigationData;
use campfire_package::{ModePackages, ModeRead};
use campfire_package_format::PackageReader;
use campfire_production::{DropOffData, NodeData, SellData};
use campfire_stats::UpgradesData;
use campfire_values::PackagePath;
use tracing::info;

use crate::attack_play::AttackPlay;
use crate::base_play::BasePlay;
use crate::decal_ground::DecalGround;
use crate::defense_play::DefensePlay;
use crate::error::CheckError;
use crate::failure::{CheckFailure, Differs};
use crate::golden::{Golden, Measured};
use crate::helicopter_play::HelicopterPlay;
use crate::jet_play::JetPlay;
use crate::match_play::PlayTools;
use crate::missile_play::MissilePlay;
use crate::pins::{
    EffectCounts, FactorySteps, FightSteps, Fingerprints, FoughtCounts, HZ, HelicopterSteps,
    LookCounts, MapCounts, PanelCounts, ProducedCounts, ProductionSteps, RangerLook, RoadCounts,
    SaleSteps, SkirmishTicks, SoundCounts, SuppliedCounts, SupplySteps, TerrainCounts,
    TerrainDrawn, UpgradeSteps, UpgradedCounts,
};
use crate::play::Play;
use crate::play_selection::PlaySelection;
use crate::skirmish_play::{SkirmishPlay, SkirmishSteps};
use crate::start_drive::StartDrive;
use crate::supply_play::SupplyPlay;

/// The checks of an import, a group at a time, each failure it finds kept: the import's report,
/// its game package, the mode of `MAP` loaded from it, the values the golden file pins, none in
/// a run that blesses, and what the checks measured of them.
#[derive(Debug)]
pub(crate) struct ImportCheck<'a> {
    pub(crate) imported: &'a Imported,
    /// The mode, read once, which each play's match loads again on the map.
    pub(crate) read: &'a ModeRead,
    pub(crate) map: &'a MapName,
    pub(crate) packages: ModePackages,
    pub(crate) golden: Option<Golden>,
    pub(crate) measured: Measured,
    pub(crate) failures: Vec<CheckFailure>,
    pub(crate) selection: &'a PlaySelection,
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

    /// Tournament Desert's heights, units, client objects, markers, terrain, roads and walls.
    pub(crate) fn map(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let heights = self
            .packages
            .heights()
            .map_or([0, 0], |grid| [grid.columns(), grid.rows()]);
        let placed = &self.packages.map().units;
        let by_type = tally(placed.iter().map(|unit| unit.unit_type.as_str()));
        let client: ClientObjects = client_file(files, &ClientObjects::path(self.map))?;
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
        let terrain: TerrainFile = client_file(files, &TerrainFile::path(self.map))?;
        let drawn = TerrainDrawn {
            quads: terrain.quads().ground.len(),
            counts: TerrainCounts {
                overlays: terrain.quads().overlays.len(),
                decals: terrain.decals().triangles.len(),
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
        if let Some(grid) = self.packages.heights() {
            let decals = DecalGround::of(grid, &terrain);
            info!(
                off = decals.off,
                most = decals.most,
                "Tournament Desert's decals on the ground"
            );
            if decals.off > 0 {
                self.failures.push(CheckFailure::OffGround(decals));
            }
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
        let pieces = self
            .imported
            .roads
            .get(self.map)
            .copied()
            .unwrap_or_default();
        let mut roads = RoadCounts {
            segments: pieces.segments,
            curves: pieces.curves,
            miters: pieces.miters,
            tees: pieces.tees,
            four_ways: pieces.four_ways,
            ys: pieces.ys,
            hs: pieces.hs,
            joins: pieces.joins,
            dropped: [0; RoadDropCause::ALL.len()],
        };
        for loss in &self.imported.losses {
            if let ImportLoss::Road { map, dropped } = loss
                && map == self.map
            {
                info!("{loss}");
                let at = RoadDropCause::ALL
                    .iter()
                    .position(|&cause| cause == dropped.cause);
                roads.dropped[at.expect("`ALL` holds every cause")] += dropped.count;
            }
        }
        let expected = self.pinned(|golden| golden.roads, roads);
        self.failures
            .extend(Differs::of(roads, expected).map(CheckFailure::Roads));
        self.measured.roads = Some(roads);
        info!(%roads, "Tournament Desert's roads");
        info!(
            samples = %format!("{} × {}", heights[0], heights[1]),
            units = counts.units,
            types = counts.unit_types,
            objects = counts.client_objects,
            looks = counts.object_looks,
            markers = counts.markers,
            quads = drawn.quads,
            overlays = drawn.counts.overlays,
            decals = drawn.counts.decals,
            copies = drawn.counts.copies,
            wraps = drawn.counts.wraps,
            untextured,
            walls = counts.walls,
            "Tournament Desert loaded"
        );
        Ok(())
    }

    /// The looks of more than one state, and the client objects' of variants, the unit looks that
    /// draw nothing, the states of every unit look, the conditions of no test, the maps of night
    /// and of snow among those the import wrote, and the animations the looks name converted and
    /// dropped, by cause.
    pub(crate) fn looks(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let unit_looks: ClientUnits = client_file(files, &ClientUnits::path())?;
        let states_of = |look: &ClientUnit| {
            let draws = look.every_draw();
            draws.map(|draw| draw.states().len()).sum::<usize>()
        };
        let varied_units = unit_looks
            .units()
            .values()
            .filter(|look| look.every_draw().any(|draw| draw.states().len() > 1))
            .count();
        let varied_objects = unit_looks.objects().values();
        let varied = varied_units
            + varied_objects
                .filter(|look| !look.variants.is_empty())
                .count();
        let empty = unit_looks
            .units()
            .values()
            .filter(|look| look.draws.is_empty())
            .count();
        let states = unit_looks.units().values().map(states_of).sum();
        let untested = unit_looks
            .units()
            .values()
            .flat_map(ClientUnit::every_draw)
            .flat_map(|draw| draw.states().iter().flat_map(|state| &state.sets))
            .filter(|set| set.untested() > 0)
            .count();
        let [night, snow] = ["night", "snow"].map(|name| DeclaredName::new(name).expect("a name"));
        let mut drawn = LookCounts {
            varied,
            empty,
            states,
            untested,
            night: 0,
            snowy: 0,
            animations: files.files_under("client/animations").count(),
            dropped_animations: self.dropped(AnimationDropCause::ALL, |loss| match loss {
                ImportLoss::Animation(animation) => Some(animation.cause),
                _ => None,
            }),
            dropped_states: StateDropCause::ALL.map(|cause| {
                let losses = self.imported.losses.iter();
                let states = losses.filter_map(|loss| match loss {
                    ImportLoss::States(dropped) if dropped.cause == cause => Some(dropped.states),
                    _ => None,
                });
                states.sum()
            }),
        };
        for other in &self.imported.maps {
            let held: MapConditions = client_file(files, &MapConditions::path(other))?;
            drawn.night += usize::from(held.conditions.contains(&night));
            drawn.snowy += usize::from(held.conditions.contains(&snow));
        }
        let expected = self.pinned(|golden| golden.looks, drawn);
        self.failures
            .extend(Differs::of(drawn, expected).map(CheckFailure::Looks));
        self.measured.looks = Some(drawn);
        info!(%drawn, "the looks of night and snow, and their animations");
        let ranger = RangerLook::of(&unit_looks);
        let expected = self.pinned(|golden| golden.ranger.clone(), ranger.clone());
        self.failures
            .extend(Differs::of(ranger.clone(), expected).map(CheckFailure::Ranger));
        info!(%ranger, "the Ranger's animations");
        self.measured.ranger = Some(ranger);
        Ok(())
    }

    /// The waypoints the import dropped of every map it wrote, which loads in the imported
    /// mode, as the import's own load checks, and its road segments of no type, which the
    /// install's road of id 1 takes (09-zero-hour.md, A road's type is `Roads.ini`'s).
    pub(crate) fn other_maps(&mut self) {
        let mut untyped = 0;
        for loss in &self.imported.losses {
            match loss {
                ImportLoss::Waypoint { .. } => info!("{loss}"),
                ImportLoss::Road { dropped, .. } if dropped.cause == RoadDropCause::NoType => {
                    info!("{loss}");
                    untyped += dropped.count;
                }
                _ => {}
            }
        }
        if untyped > 0 {
            self.failures.push(CheckFailure::UntypedRoads(untyped));
        }
        info!(maps = self.imported.maps.len(), "the imported maps loaded");
    }

    /// The trains and builds of every command set, the unit types that sell, and the buttons
    /// dropped, by cause.
    pub(crate) fn production(&mut self) {
        let units = self.packages.content().units.values();
        let sells = units.filter(|unit| unit.section::<SellData>().is_some());
        let produced = ProducedCounts {
            trains: self.kind_count(ActionKind::Train),
            builds: self.kind_count(ActionKind::Build),
            sells: sells.count(),
            dropped: self.dropped(ButtonDropCause::ALL, |loss| match loss {
                ImportLoss::Button(button) => Some(button.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.produced, produced);
        self.failures
            .extend(Differs::of(produced, expected).map(CheckFailure::Produced));
        self.measured.produced = Some(produced);
        info!(
            trains = produced.trains,
            builds = produced.builds,
            sells = produced.sells,
            dropped = ?produced.dropped,
            "the command sets' trains, builds and sales"
        );
    }

    /// The researches of every command set, the upgrade modules the unit types take, and the
    /// modules and objects of sets dropped, by cause.
    pub(crate) fn upgrades(&mut self) {
        let units = self.packages.content().units.values();
        let modules = units
            .filter_map(|unit| unit.section::<UpgradesData>())
            .map(|upgrades| upgrades.0.len())
            .sum();
        let upgraded = UpgradedCounts {
            researches: self.kind_count(ActionKind::Research),
            modules,
            dropped: self.dropped(UpgradeDropCause::ALL, |loss| match loss {
                ImportLoss::Upgrade(module) => Some(module.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.upgraded, upgraded);
        self.failures
            .extend(Differs::of(upgraded, expected).map(CheckFailure::Upgraded));
        self.measured.upgraded = Some(upgraded);
        info!(%upgraded, "the researches and the upgrade modules");
    }

    /// The docks' nodes, the supply centers' drop-offs, the gatherers' gathers, the hackers'
    /// hacks, each action whose channel pays, and the supply dropped, by cause.
    pub(crate) fn supply(&mut self) {
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
            dropped: self.dropped(SupplyDropCause::ALL, |loss| match loss {
                ImportLoss::Supply(supply) => Some(supply.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.supplied, supplied);
        self.failures
            .extend(Differs::of(supplied, expected).map(CheckFailure::Supplied));
        self.measured.supplied = Some(supplied);
        info!(
            nodes = supplied.nodes,
            drop_offs = supplied.drop_offs,
            gathers = supplied.gathers,
            hacks = supplied.hacks,
            dropped = ?supplied.dropped,
            "the docks, supply centers, gatherers and hackers"
        );
    }

    /// The panel: the actions' icons and keys, the looks' places, the grid's commands of an icon,
    /// and the buttons dropped or shown in part, by cause.
    pub(crate) fn panel(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let units: ClientUnits = client_file(files, &ClientUnits::path())?;
        let hud: HudFile = client_file(files, &HudFile::path())?;
        let actions = units.actions().values();
        let looks = units.units().values().filter(|look| !look.panel.is_empty());
        let panel = PanelCounts {
            icons: actions.clone().filter(|look| look.icon.is_some()).count(),
            keys: actions.filter(|look| look.key.is_some()).count(),
            panels: looks.clone().count(),
            places: looks.map(|look| look.panel.iter().count()).sum(),
            commands: (hud.panel.commands.values())
                .filter(|look| look.icon.is_some())
                .count(),
            dropped: self.dropped(PanelDropCause::ALL, |loss| match loss {
                ImportLoss::Panel(button) => Some(button.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.panel, panel);
        self.failures
            .extend(Differs::of(panel, expected).map(CheckFailure::Panel));
        self.measured.panel = Some(panel);
        info!(%panel, "the command panel");
        Ok(())
    }

    /// The sounds, the lists, the fires and the looks that name them, the files of the sounds and
    /// of the music, the playlists, and the sounds dropped, by cause.
    pub(crate) fn sounds(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let effects: EffectsFile = client_file(files, &EffectsFile::path())?;
        let units: ClientUnits = client_file(files, &ClientUnits::path())?;
        let sound: SoundFile = client_file(files, &SoundFile::path())?;
        let lists = effects.lists().values();
        let sounds = SoundCounts {
            sounds: effects.sounds().len(),
            lists: lists.filter(|list| !list.sounds.is_empty()).count(),
            fires: (units.actions().values())
                .filter(|look| look.fire_sound.is_some())
                .count(),
            voiced: (units.units().values())
                .filter(|look| !look.voices.is_empty())
                .count(),
            files: files.files_under("client/sounds").count(),
            tracks: files.files_under("client/music").count(),
            playlists: sound.music.map_or(0, |music| music.playlists.len()),
            dropped: self.dropped(SoundDropCause::ALL, |loss| match loss {
                ImportLoss::Sound(event) => Some(event.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.sounds, sounds);
        self.failures
            .extend(Differs::of(sounds, expected).map(CheckFailure::Sounds));
        self.measured.sounds = Some(sounds);
        info!(%sounds, "the sounds");
        Ok(())
    }

    /// The particle systems and effect lists, the textures they draw, the effects the actions, the
    /// looks and their states name, and the effects dropped, by cause.
    pub(crate) fn effects(&mut self) -> Result<(), CheckError> {
        let files = &self.packages.mode().package.files;
        let file: EffectsFile = client_file(files, &EffectsFile::path())?;
        let textures: BTreeSet<_> = file
            .systems()
            .values()
            .map(|system| &system.texture)
            .collect();
        let units: ClientUnits = client_file(files, &ClientUnits::path())?;
        let actions = units.actions().values();
        let named = |of: fn(&ActionLook) -> bool| actions.clone().filter(|look| of(look)).count();
        let looks = units.units().values();
        let states = looks
            .clone()
            .flat_map(ClientUnit::every_draw)
            .flat_map(LookDraw::states);
        let effects = EffectCounts {
            systems: file.systems().len(),
            lists: file.lists().len(),
            textures: textures.len(),
            fires: named(|look| look.fire.is_some()),
            hits: named(|look| look.hit.is_some()),
            trails: looks.clone().filter(|look| look.trail.is_some()).count(),
            ends: looks.clone().filter(|look| look.end.is_some()).count(),
            deaths: looks.filter(|look| !look.death.is_empty()).count(),
            bones: states.filter(|state| !state.fire_bones.is_empty()).count(),
            dropped: self.dropped(EffectDropCause::ALL, |loss| match loss {
                ImportLoss::Effect(effect) => Some(effect.cause),
                _ => None,
            }),
        };
        let expected = self.pinned(|golden| golden.effects, effects);
        self.failures
            .extend(Differs::of(effects, expected).map(CheckFailure::Effects));
        self.measured.effects = Some(effects);
        info!(%effects, "the particle systems and effect lists");
        Ok(())
    }

    /// The weapons, the armors, the damage kinds and the bodies of no life or a wounded start.
    pub(crate) fn combat(&mut self) {
        let combat = self.packages.data().section_or_default::<ModeCombatData>();
        let losses = &self.imported.losses;
        let count = |kind: fn(&ImportLoss) -> bool| losses.iter().filter(|loss| kind(loss)).count();
        let fought = FoughtCounts {
            weapons: self.kind_count(ActionKind::Attack),
            dropped: self.dropped(WeaponDropCause::ALL, |loss| match loss {
                ImportLoss::Weapon(weapon) => Some(weapon.cause),
                _ => None,
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
        info!(%fought, "the weapons, armors and bodies");
    }

    /// The packages of a match of `play`, loaded again, and its tools, its probe resolved in them
    /// at `rate` before the play runs; none when the run does not play it, or its probe fails,
    /// which goes to the failures.
    fn played(
        &mut self,
        play: Play,
        rate: NonZeroU32,
    ) -> Result<Option<(ModePackages, PlayTools)>, CheckError> {
        if !self.selection.runs(play) {
            return Ok(None);
        }
        let packages = self.read.on(Some(self.map)).map_err(CheckError::Load)?;
        let tools = Self::tools(self.selection, play, &packages, rate);
        let tools = tools.map_err(|failure| self.failures.push(failure)).ok();
        Ok(tools.map(|tools| (packages, tools)))
    }

    /// The tools `selection` gives `play` in a match of `packages` at `rate`: its probe, if it
    /// asks one, and its record; or why its probe fails.
    fn tools(
        selection: &PlaySelection,
        play: Play,
        packages: &ModePackages,
        rate: NonZeroU32,
    ) -> Result<PlayTools, CheckFailure> {
        let probe = selection.probe(play);
        let probe = probe.map(|probe| probe.resolve(packages.load_books(), rate));
        Ok(PlayTools {
            probe: probe.transpose()?,
            record: selection.record(),
        })
    }

    /// The skirmish's production and fight, in a match of its own of the packages loaded again,
    /// then the start and the dozer's drive, at `HZ` ticks a second; then, in a run that blesses,
    /// the golden file written of what the checks measured: every failure the checks found.
    pub(crate) fn plays(mut self) -> Result<Vec<CheckFailure>, CheckError> {
        let rate = u32::try_from(HZ).ok().and_then(NonZeroU32::new);
        let rate = rate.expect("a rate");
        if let Some((played, tools)) = self.played(Play::Skirmish, rate)?
            && let Some(SkirmishSteps {
                production,
                factory,
                strategy_center,
                upgrades,
                sale,
                fight,
                bot,
            }) = SkirmishPlay::run(played, rate, tools, &mut self.failures)
        {
            let ticks = SkirmishTicks {
                barracks: production.barracks_ticks,
                factory: factory.ticks,
                strategy_center,
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
            let expected = UpgradeSteps::expected();
            self.failures
                .extend(Differs::of(upgrades, expected).map(CheckFailure::Upgrades));
            info!(
                strategy_center,
                research = upgrades.research_ticks,
                money = upgrades.money,
                crusader_life = ?upgrades.crusader_life,
                ranger_tags = ?upgrades.ranger_tags,
                ranger_weapons = ?upgrades.ranger_weapons,
                rods = upgrades.rods,
                "player 1 built a strategy center and researched its upgrades"
            );
            self.failures
                .extend(Differs::of(sale, SaleSteps::expected()).map(CheckFailure::Sale));
            info!(
                ticks = sale.ticks,
                refund = sale.refund,
                "player 1 sold its power plant"
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
        if let Some((played, tools)) = self.played(Play::Supply, rate)?
            && let Some(supply) = SupplyPlay::run(played, rate, tools, &mut self.failures)
        {
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
        if let Some((played, tools)) = self.played(Play::Helicopter, rate)?
            && let Some(helicopter) = HelicopterPlay::run(played, rate, tools, &mut self.failures)
        {
            let ticks = helicopter.ticks;
            let expected =
                HelicopterSteps::expected(self.pinned(|golden| golden.helicopter, ticks));
            self.failures
                .extend(Differs::of(helicopter, expected).map(CheckFailure::Helicopters));
            self.measured.helicopter = Some(ticks);
            info!(
                ticks = ?helicopter.ticks,
                load = helicopter.load,
                "player 1's Comanche climbed, its Chinook delivered a load, and GLA's base shot the \
                 Comanche down"
            );
        }
        if let Some((played, tools)) = self.played(Play::Jet, rate)?
            && let Some(jet) = JetPlay::run(played, rate, tools, &mut self.failures)
        {
            let expected = self.pinned(|golden| golden.jet, jet);
            self.failures
                .extend(Differs::of(jet, expected).map(CheckFailure::Jet));
            self.measured.jet = Some(jet);
            info!(
                ticks = ?jet,
                "player 1's Raptor took off, struck GLA's command center, landed and reloaded"
            );
        }
        if let Some((played, tools)) = self.played(Play::Gattling, rate)?
            && let Some(defense) =
                DefensePlay::GATTLING.run(played, rate, tools, &mut self.failures)
        {
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
        if let Some((played, tools)) = self.played(Play::Patriot, rate)?
            && let Some(patriot) = DefensePlay::PATRIOT.run(played, rate, tools, &mut self.failures)
        {
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
        if let Some((played, tools)) = self.played(Play::Stinger, rate)?
            && let Some(stinger) = DefensePlay::STINGER.run(played, rate, tools, &mut self.failures)
        {
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
        if let Some((played, tools)) = self.played(Play::Missile, rate)?
            && let Some(missile) = MissilePlay::run(played, rate, tools, &mut self.failures)
        {
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
        if let Some((played, tools)) = self.played(Play::Base, rate)?
            && let Some(base) = BasePlay::run(played, rate, tools, &mut self.failures)
        {
            let expected = self.pinned(|golden| golden.base, base);
            self.failures
                .extend(Differs::of(base, expected).map(CheckFailure::Base));
            self.measured.base = Some(base);
            info!(
                ended = ?base.ended,
                result = ?base.result,
                standing = ?base.standing,
                drops = ?base.drops,
                "two bots started with their bases, and fought"
            );
        }
        if let Some((played, tools)) = self.played(Play::Attack, rate)?
            && let Some(attack) = AttackPlay::run(played, rate, tools, &mut self.failures)
        {
            let expected = self.pinned(|golden| golden.attack, attack);
            self.failures
                .extend(Differs::of(attack, expected).map(CheckFailure::Attack));
            self.measured.attack = Some(attack);
            info!(
                ended = ?attack.ended,
                result = ?attack.result,
                standing = ?attack.standing,
                drops = ?attack.drops,
                "the attacker bot built its base and fought the builder bot"
            );
        }
        let selection = self.selection;
        if selection.runs(Play::Drive)
            && let Some(tools) = Self::tools(selection, Play::Drive, &self.packages, rate)
                .map_err(|failure| self.failures.push(failure))
                .ok()
            && let Some(drive) = StartDrive::run(self.packages, rate, tools, &mut self.failures)
        {
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

    /// How many of the losses `kind` picks hold each cause of `all`, in its order, each loss
    /// logged.
    fn dropped<C: Copy + PartialEq, const N: usize>(
        &self,
        all: [C; N],
        kind: fn(&ImportLoss) -> Option<C>,
    ) -> [usize; N] {
        let mut counts = [0; N];
        for loss in &self.imported.losses {
            if let Some(cause) = kind(loss) {
                info!("{loss}");
                let at = all.iter().position(|&each| each == cause);
                counts[at.expect("a cause's `ALL` holds it")] += 1;
            }
        }
        counts
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

/// The client file `F` at `path` of `files`, read and decoded in the format its type declares, as
/// the client reads it (02-engine-core.md, A client file declares its format).
fn client_file<F: ClientFile>(files: &PackageReader, path: &PackagePath) -> Result<F, CheckError> {
    F::decode(&files.read_file(path)?).map_err(CheckError::ClientFile)
}
