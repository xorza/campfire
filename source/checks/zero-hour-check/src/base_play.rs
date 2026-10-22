use std::num::NonZeroU32;

use campfire_capabilities::Match;
use campfire_common::DeclaredName;
use campfire_geometry::Placed;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_mode::{ListEntry, ModeParam, OptionPicks, OptionValue, SeatPick};
use campfire_package::ModePackages;
use campfire_production::Site;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_sim::Position;
use campfire_units::{Body, Life, MoveStep, Units, WallBook};
use campfire_values::{Scalar, UnitType};

use crate::failure::CheckFailure;
use crate::match_play::{MatchPlay, START_MONEY};
use crate::pins::{BotMatchEnd, HZ};

/// The most ticks the bots' match runs.
const BASE_LIMIT: u64 = HZ * 60 * 20;

/// A seat of the base play: it starts with its faction's base at its start, `Player_<n>_Start`
/// of its `start`, and the mode's attacker plays it (09-zero-hour.md, A base start).
#[derive(Debug, Clone, Copy)]
struct BaseBot {
    seat: u32,
    faction: &'static str,
    start: u32,
}

/// China at Tournament Desert's first start, GLA at its second, whose gatherers walk.
const BOTS: [BaseBot; 2] = [
    BaseBot {
        seat: 0,
        faction: "china",
        start: 1,
    },
    BaseBot {
        seat: 1,
        faction: "gla",
        start: 2,
    },
];

/// A faction's base, as the imported mode's params give it: its base buildings, its base units,
/// and how many of each a seat starts with.
#[derive(Debug)]
struct Base {
    buildings: Vec<String>,
    units: Vec<String>,
    count: usize,
}

/// The base play on Tournament Desert of `START_MONEY` (09-zero-hour.md, A base start;
/// 09-zero-hour.md, A bots' match that ends): two bots, China and GLA, each start with
/// its faction's base, every base building whole and `base_count` of each base unit, no body on
/// another or on a wall; then the mode's attacker plays each, until one side's last building that
/// counts falls or `BASE_LIMIT` passes, and the play pins how the match stood then.
#[derive(Debug)]
pub(crate) struct BasePlay;

impl BasePlay {
    /// Plays the bases in a match of `packages` at `rate` ticks a second: how the match stood at
    /// its end or its limit, or none, with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<BotMatchEnd> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
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
        let closed = play.end();
        match ended.and_then(|ended| closed.map(|()| ended)) {
            Ok(ended) => Some(ended),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The bases in `play`, of `packages`, after its start's tick, then the bots' match.
    fn play(
        play: &mut MatchPlay,
        packages: &ModePackages,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<BotMatchEnd, CheckFailure> {
        for bot in BOTS {
            let base = Base::of(packages, bot.faction)?;
            base.check(play.game(), bot.seat, failures)?;
            let home = play.start(bot.start)?;
            play.drive_mode_bot(packages, bot.seat, "attacker", home)?;
        }
        let overlaps = overlaps(play.game());
        if overlaps > 0 {
            failures.push(CheckFailure::BaseOverlaps(overlaps));
        }
        Ok(play.fight(BASE_LIMIT))
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
        let count = DeclaredName::new("base_count").expect("a param's name");
        let count = match params.get(&count) {
            Some(ModeParam::Value(Scalar::Int(count))) => usize::try_from(*count).ok(),
            _ => None,
        };
        Ok(Self {
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
