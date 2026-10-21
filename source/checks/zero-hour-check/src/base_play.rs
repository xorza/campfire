use std::fmt::Write as _;
use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_actions::TypeSlotsData;
use campfire_capabilities::Match;
use campfire_common::DeclaredName;
use campfire_geometry::Placed;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Vec3;
use campfire_mode::internals::MarkerOf;
use campfire_mode::{ListEntry, Mode, ModeParam, OptionPicks, OptionValue, SeatPick};
use campfire_package::ModePackages;
use campfire_production::Site;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_sim::Position;
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;
use campfire_units::{Body, Life, MoveStep, Units, WallBook};
use campfire_values::{Scalar, UnitType};

use crate::error::CheckError;
use crate::failure::CheckFailure;
use crate::match_play::{self, MatchPlay, START_MONEY};
use crate::pins::{BotMatchEnd, HZ};

/// The ticks between a bot's rounds of trains: each producer queues its next train.
const TRAIN_EVERY: u64 = HZ * 20;
/// The ticks between a bot's moves of its army into the other start.
const MOVE_EVERY: u64 = HZ * 40;
/// The ticks after each move that the army attacks the nearest enemy it sees.
const ATTACK_AFTER: u64 = HZ * 20;
/// The most ticks the bots' match runs, and the last of their orders: their armies wear each other
/// down to a stand within it.
const BASE_LIMIT: u64 = HZ * 60 * 20;
/// The most bytes a bot's file may hold.
const MAX_SCRIPT: usize = 1 << 20;

/// A seat of the base play: it starts with its faction's base at its start, and its bot moves its
/// army into the other start (09-zero-hour.md, Bots that fight from a base).
#[derive(Debug, Clone, Copy)]
struct BaseBot {
    seat: u32,
    faction: &'static str,
    /// Its faction's name as a reader reads it.
    title: &'static str,
    /// Its start's number, `n` of `Player_<n>_Start`, and the other start's.
    start: u32,
    enemy: u32,
}

/// China at Tournament Desert's first start, America at its second.
const BOTS: [BaseBot; 2] = [
    BaseBot {
        seat: 0,
        faction: "china",
        title: "China",
        start: 1,
        enemy: 2,
    },
    BaseBot {
        seat: 1,
        faction: "america",
        title: "America",
        start: 2,
        enemy: 1,
    },
];

/// A faction's base, as the imported mode's params give it: its faction building, its base
/// buildings, its base units, and how many of each a seat starts with.
#[derive(Debug)]
struct Base {
    building: String,
    buildings: Vec<String>,
    units: Vec<String>,
    count: usize,
}

/// The base play on Tournament Desert of `START_MONEY` (09-zero-hour.md, A base start;
/// 09-zero-hour.md, Bots that fight from a base): two bots, China and America, each start with
/// its faction's base, every base building whole and `base_count` of each base unit, no body on
/// another or on a wall; then each trains its units and sends its army at the other until the
/// match ends or `BASE_LIMIT` passes, and the play pins how it stood then. The check makes each
/// bot's orders from the imported mode, and keeps them in `bots/`, where a host takes them for a
/// local server's bots.
#[derive(Debug)]
pub(crate) struct BasePlay;

impl BasePlay {
    /// Plays the bases in a match of `packages` at `rate` ticks a second: how the match stood at
    /// its end or its limit, or none, with each failure in `failures`; an error when a run that
    /// blesses does not write a bot's file.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<Option<BotMatchEnd>, CheckError> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return Ok(None);
            }
        };
        let seat_picks: Vec<SeatPick> = BOTS
            .iter()
            .flat_map(|bot| {
                [
                    format!("{}:faction={}", bot.seat, bot.faction),
                    format!("{}:start=base", bot.seat),
                ]
            })
            .map(|pick| pick.parse().expect("a base seat's pick reads"))
            .collect();
        let session = FixedSession::planned(packages, InputRules::ROOMY, [SeatPlan::Bot; 2])
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&seat_picks);
        let mut play = MatchPlay::new(session.start());
        let ended = Self::play(&mut play, session.packages(), failures);
        play.end();
        match ended {
            Ok(ended) => Ok(ended),
            Err(BaseFault::Failure(failure)) => {
                failures.push(failure);
                Ok(None)
            }
            Err(BaseFault::Error(error)) => Err(error),
        }
    }

    /// The bases in `play`, of `packages`, after its start's tick, then the bots' match.
    fn play(
        play: &mut MatchPlay,
        packages: &ModePackages,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<Option<BotMatchEnd>, BaseFault> {
        for bot in BOTS {
            let base = Base::of(packages, bot.faction)?;
            base.check(play.game(), bot.seat, failures)?;
            let marker = |start: u32| {
                let name = format!("Player_{start}_Start");
                let of = MarkerOf {
                    tag: "start",
                    param: "name",
                    value: &name,
                };
                Mode::marker_point(play.game(), of).ok_or(CheckFailure::NoStarts)
            };
            let (home, enemy) = (marker(bot.start)?, marker(bot.enemy)?);
            let text = bot.script(packages, &base, enemy)?;
            bot.pin(&text, failures)?;
            play.drive_bot(bot.seat, match_play::bot_script(packages, &text)?, home);
        }
        let overlaps = overlaps(play.game());
        if overlaps > 0 {
            failures.push(CheckFailure::BaseOverlaps(overlaps));
        }
        Ok(Some(play.fight(BASE_LIMIT)))
    }
}

/// What ends the base play: a failure of the import, or an error of the run.
#[derive(Debug)]
enum BaseFault {
    Failure(CheckFailure),
    Error(CheckError),
}

impl From<CheckFailure> for BaseFault {
    fn from(failure: CheckFailure) -> Self {
        Self::Failure(failure)
    }
}

impl Base {
    /// The base of `faction` in `packages`; a failure when the mode lacks one of its params.
    fn of(packages: &ModePackages, faction: &'static str) -> Result<Self, CheckFailure> {
        let params = &packages.data().params;
        let param = |key: &str| {
            let name = DeclaredName::new(&format!("{key}_{faction}")).expect("a param's name");
            params.get(&name).ok_or(CheckFailure::NoBase(faction))
        };
        let list = |key: &str| match param(key)? {
            ModeParam::List(entries) => entries
                .iter()
                .map(|entry| match entry {
                    ListEntry::Text(text) => Ok(text.clone()),
                    ListEntry::Value(_) => Err(CheckFailure::NoBase(faction)),
                })
                .collect(),
            _ => Err(CheckFailure::NoBase(faction)),
        };
        let ModeParam::Text(building) = param("start_building")? else {
            return Err(CheckFailure::NoBase(faction));
        };
        let count = DeclaredName::new("base_count").expect("a param's name");
        let count = match params.get(&count) {
            Some(ModeParam::Value(Scalar::Int(count))) => usize::try_from(*count).ok(),
            _ => None,
        };
        Ok(Self {
            building: building.clone(),
            buildings: list("base_buildings")?,
            units: list("base_units")?,
            count: count.ok_or(CheckFailure::NoBase(faction))?,
        })
    }

    /// How many living units of the type `name`, each whole, player `seat` of `world` owns; a
    /// failure when the mode has no such type.
    fn count(world: &Match, name: &str, seat: u32) -> Result<usize, CheckFailure> {
        let typed: UnitType = Units::type_named(world, name)
            .ok_or_else(|| CheckFailure::BaseType(name.to_owned()))?;
        let held = MatchUnits::of(world).all().filter(|(_, unit)| {
            unit.get::<UnitType>() == Some(&typed)
                && MatchUnits::owned_by(unit, seat)
                && !unit.contains::<Site>()
                && unit.get::<Life>().is_none_or(|life| !life.is_empty())
        });
        Ok(held.count())
    }

    /// The failures of the base player `seat` of `world` started with: a base building it holds
    /// none of whole, or a base unit it holds fewer than its count of.
    fn check(
        &self,
        world: &Match,
        seat: u32,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<(), CheckFailure> {
        let count = |name: &str| Self::count(world, name, seat);
        for building in &self.buildings {
            if count(building)? == 0 {
                failures.push(CheckFailure::BaseBuilding {
                    seat,
                    unit: building.clone(),
                });
            }
        }
        for unit in &self.units {
            let held = count(unit)?;
            if held < self.count {
                failures.push(CheckFailure::BaseUnit {
                    seat,
                    unit: unit.clone(),
                    held,
                });
            }
        }
        Ok(())
    }
}

impl BaseBot {
    /// Its file among the sample bots, by its start, its faction and its role.
    fn path(self) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bots/zero-hour/tournament_desert"
        ))
        .join(format!(
            "start{}-{}-base-attacker.toml",
            self.start, self.faction
        ))
    }

    /// Its orders, of `base` in `packages`, its army sent at `enemy`: every `TRAIN_EVERY` ticks,
    /// each of its producers queues the next of its trains of a unit that neither builds nor
    /// gathers, in turn; every `MOVE_EVERY` ticks, its army, each such base unit type, moves to
    /// `enemy`, and
    /// `ATTACK_AFTER` ticks later attacks the nearest enemy its first unit sees.
    fn script(
        self,
        packages: &ModePackages,
        base: &Base,
        enemy: Vec3,
    ) -> Result<String, CheckFailure> {
        let command = DeclaredName::new("command").expect("a declared name");
        let slots = |unit: &str| -> Vec<String> {
            let held = packages.content().units.get(unit);
            let slots = held.and_then(|held| held.section::<TypeSlotsData>()?.0.get(&command));
            slots
                .into_iter()
                .flatten()
                .map(|slot| slot.as_str().to_owned())
                .collect()
        };
        let fights = |unit: &str| {
            !slots(unit)
                .iter()
                .any(|slot| slot.starts_with("build_") || slot.starts_with("gather_"))
        };
        let producers: Vec<(&str, Vec<String>)> = [&base.building]
            .into_iter()
            .chain(&base.buildings)
            .map(|producer| {
                let trains = slots(producer)
                    .into_iter()
                    .filter(|slot| slot.strip_prefix("train_").is_some_and(&fights));
                (producer.as_str(), trains.collect::<Vec<_>>())
            })
            .filter(|(_, trains)| !trains.is_empty())
            .collect();
        let army: Vec<&str> = base
            .units
            .iter()
            .map(String::as_str)
            .filter(|unit| fights(unit))
            .collect();
        if army.is_empty() {
            return Err(CheckFailure::NoBase(self.faction));
        }
        let mut orders: Vec<(u64, String)> = Vec::new();
        let rounds = (TRAIN_EVERY..BASE_LIMIT).step_by(TRAIN_EVERY as usize);
        for (round, tick) in rounds.enumerate() {
            for (producer, trains) in &producers {
                let train = &trains[round % trains.len()];
                orders.push((
                    tick,
                    format!(
                        "units = \"{producer}\"\n\
                         directive.orders.slot = {{ slot = \"{train}\", target = \"none\" }}\n"
                    ),
                ));
            }
        }
        let [x, z] = [enemy.x.round(), enemy.z.round()];
        for tick in (MOVE_EVERY..BASE_LIMIT).step_by(MOVE_EVERY as usize) {
            for unit in &army {
                orders.push((
                    tick,
                    format!("units = \"{unit}\"\ndirective.orders.move.to = [{x}, {z}]\n"),
                ));
                orders.push((
                    tick + ATTACK_AFTER,
                    format!("units = \"{unit}\"\ndirective.orders.attack.target = \"enemies\"\n"),
                ));
            }
        }
        orders.sort_by_key(|&(tick, _)| tick);
        let mut text = format!(
            "# The base attacker of {title} at Tournament Desert's start {start}, \
             `Player_{start}_Start`. It starts\n# with its faction's base and builds nothing: \
             every {train} s each producer of its base queues its next\n# train, and every \
             {every} s its army moves to start {enemy}, ({x}, {z}), and attacks the nearest \
             enemy\n# {attack} s later. The Zero Hour check makes it from the imported mode: \
             `CAMPFIRE_BLESS=1 cargo drive run\n# zero-hour` writes it again. Play it with \
             `--server-bot {seat}=<this file> --pick {seat}:faction={faction}\n# --pick \
             {seat}:start=base --option {START_MONEY}`.\n",
            faction = self.faction,
            title = self.title,
            start = self.start,
            enemy = self.enemy,
            seat = self.seat,
            train = TRAIN_EVERY / HZ,
            every = MOVE_EVERY / HZ,
            attack = ATTACK_AFTER / HZ,
        );
        for (tick, order) in orders {
            write!(text, "\n[[order]]\ntick = {tick}\n{order}").expect("text writes");
        }
        Ok(text)
    }

    /// Writes `text` into its file in a run that blesses; else a failure when the file holds
    /// other than `text`.
    fn pin(self, text: &str, failures: &mut Vec<CheckFailure>) -> Result<(), BaseFault> {
        let path = self.path();
        if PinnedFile::is_blessing() {
            PinnedFile::new(&path)
                .bless(text.as_bytes())
                .map_err(|error| BaseFault::Error(CheckError::BotWrite(error)))?;
            return Ok(());
        }
        let held = InputFile::read(&path, MAX_SCRIPT).ok();
        if held.as_deref() != Some(text.as_bytes()) {
            failures.push(CheckFailure::BotFile(self.faction));
        }
        Ok(())
    }
}

/// How many bodies of the two seats overlap a wall of their layer, and how many pairs of standing
/// bodies of one layer, one of them a seat's, overlap: a walker its building frees or keeps leaves
/// through the building's exit, as a trained unit does, and navigation pushes walkers apart.
fn overlaps(world: &Match) -> usize {
    let bodies: Vec<_> = MatchUnits::of(world)
        .all()
        .filter_map(|(_, unit)| {
            let body = unit.get::<Body>()?;
            let position = unit.get::<Position>()?.get();
            let seat = (0..2).any(|seat| MatchUnits::owned_by(&unit, seat));
            let stands = !unit.contains::<MoveStep>();
            Some((
                Placed::new(position, body.shape()),
                body.layer(),
                seat,
                stands,
            ))
        })
        .collect();
    let walls = world.resource::<WallBook>();
    let on_walls = bodies
        .iter()
        .filter(|&&(placed, layer, seat, _)| seat && walls.overlaps(placed, layer))
        .count();
    let standing: Vec<_> = bodies.iter().filter(|body| body.3).collect();
    let mut pairs = 0;
    for (at, &&(placed, layer, seat, _)) in standing.iter().enumerate() {
        pairs += standing[at + 1..]
            .iter()
            .filter(|&&&(other, other_layer, other_seat, _)| {
                (seat || other_seat) && layer == other_layer && placed.overlaps(other)
            })
            .count();
    }
    on_walls + pairs
}
