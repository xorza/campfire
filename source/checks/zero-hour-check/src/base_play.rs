use std::num::NonZeroU32;

use campfire_capabilities::Match;
use campfire_common::DeclaredName;
use campfire_geometry::Placed;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_mode::{ListEntry, ModeParam, OptionPicks, OptionValue, PlayerSlotPick};
use campfire_package::ModePackages;
use campfire_production::Site;
use campfire_protocol::PlayerSlotPlan;
use campfire_sim::Position;
use campfire_units::{Body, Life, MoveStep, Units, WallBook};
use campfire_values::{Scalar, UnitType};

use crate::failure::CheckFailure;
use crate::match_play::{MatchPlay, PlayTools, START_MONEY};
use crate::pins::{BotMatchEnd, HZ};

/// The most ticks the bots' match runs.
const BASE_LIMIT: u64 = HZ * 60 * 20;

/// A player slot of the base play: it starts with its faction's base at its start,
/// `Player_<n>_Start` of its `start`, and the mode's attacker plays it (09-zero-hour.md, A base
/// start).
#[derive(Debug, Clone, Copy)]
struct BaseBot {
    player_slot: u32,
    faction: &'static str,
    start: u32,
}

/// China at Tournament Desert's first start, GLA at its second, whose gatherers walk.
const BOTS: [BaseBot; 2] = [
    BaseBot {
        player_slot: 0,
        faction: "china",
        start: 1,
    },
    BaseBot {
        player_slot: 1,
        faction: "gla",
        start: 2,
    },
];

/// A faction's base, as the imported mode's params give it: its base buildings, its base units,
/// and how many of each a player slot starts with.
#[derive(Debug)]
struct Base {
    buildings: Vec<DeclaredName>,
    units: Vec<DeclaredName>,
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
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<BotMatchEnd> {
        let played = Self::session(packages, rate).and_then(|session| {
            Self::with_play(&session, tools, failures, |play| play.fight(BASE_LIMIT))
        });
        match played {
            Ok(ended) => Some(ended),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The session of the bases play of `packages` at `rate` ticks a second, which starts each of
    /// its matches.
    pub(crate) fn session(
        packages: ModePackages,
        rate: NonZeroU32,
    ) -> Result<FixedSession, CheckFailure> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = OptionPicks::named(&packages.data().options, &[start_money])
            .map_err(CheckFailure::Options)?;
        let player_slot_picks: Vec<PlayerSlotPick> = BOTS
            .iter()
            .flat_map(|bot| {
                [
                    format!("{}:faction={}", bot.player_slot, bot.faction),
                    format!("{}:start=base", bot.player_slot),
                ]
            })
            .map(|pick| pick.parse().expect("a base player slot's pick reads"))
            .collect();
        let session = FixedSession::planned(packages, [PlayerSlotPlan::Bot; 2])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&player_slot_picks);
        Ok(session)
    }

    /// Starts the bases in a match of `session`, its bots driven, runs `steps` on it, and ends
    /// it: what `steps` gave, or the play's failure, with each failure of the bases in
    /// `failures`.
    pub(crate) fn with_play<T>(
        session: &FixedSession,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
        steps: impl FnOnce(&mut MatchPlay<'_>) -> T,
    ) -> Result<T, CheckFailure> {
        let mut play = MatchPlay::new(session.start(), session.packages(), tools);
        let stepped =
            Self::start(&mut play, session.packages(), failures).map(|()| steps(&mut play));
        let closed = play.end();
        let stepped = stepped?;
        closed?;
        Ok(stepped)
    }

    /// The bases in `play`, of `packages`, after its start's tick, each checked and driven by the
    /// mode's attacker.
    fn start(
        play: &mut MatchPlay<'_>,
        packages: &ModePackages,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<(), CheckFailure> {
        for bot in BOTS {
            let base = Base::of(packages, bot.faction)?;
            base.check(play.game(), bot.player_slot, failures)?;
            let home = play.start(bot.start)?;
            play.drive_mode_bot(packages, bot.player_slot, "attacker", home)?;
        }
        let overlaps = overlaps(play.game());
        if overlaps > 0 {
            failures.push(CheckFailure::BaseOverlaps(overlaps));
        }
        Ok(())
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
                    ListEntry::Name(name) => Ok(name.clone()),
                    ListEntry::Value(_) | ListEntry::Text(_) => Err(CheckFailure::NoBase(faction)),
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

    /// How many living units of the type `name`, each whole, player `player_slot` of `world` owns;
    /// a failure when the mode has no such type.
    fn count(world: &Match, name: &DeclaredName, player_slot: u32) -> Result<usize, CheckFailure> {
        let typed: UnitType = Units::type_named(world, name.as_str())
            .ok_or_else(|| CheckFailure::BaseType(name.clone()))?;
        let held = MatchUnits::of(world).all().filter(|(_, unit)| {
            unit.get::<UnitType>() == Some(&typed)
                && MatchUnits::owned_by(unit, player_slot)
                && !unit.contains::<Site>()
                && unit.get::<Life>().is_none_or(|life| !life.is_empty())
        });
        Ok(held.count())
    }

    /// The failures of the base player `player_slot` of `world` started with: a base building it
    /// holds none of whole, or a base unit it holds fewer than its count of.
    fn check(
        &self,
        world: &Match,
        player_slot: u32,
        failures: &mut Vec<CheckFailure>,
    ) -> Result<(), CheckFailure> {
        let count = |name: &DeclaredName| Self::count(world, name, player_slot);
        for building in &self.buildings {
            if count(building)? == 0 {
                failures.push(CheckFailure::BaseBuilding {
                    player_slot,
                    unit: building.clone(),
                });
            }
        }
        for unit in &self.units {
            let held = count(unit)?;
            if held < self.count {
                failures.push(CheckFailure::BaseUnit {
                    player_slot,
                    unit: unit.clone(),
                    held,
                });
            }
        }
        Ok(())
    }
}

/// How many bodies of the two player slots overlap a wall of their layer, and how many pairs of
/// standing bodies of one layer, one of them a player slot's, overlap: a walker its building frees
/// or keeps leaves through the building's exit, as a trained unit does, and navigation pushes
/// walkers apart.
fn overlaps(world: &Match) -> usize {
    let bodies: Vec<_> = MatchUnits::of(world)
        .all()
        .filter_map(|(_, unit)| {
            let body = unit.get::<Body>()?;
            let position = unit.get::<Position>()?.get();
            let owned = (0..2).any(|player_slot| MatchUnits::owned_by(&unit, player_slot));
            let stands = !unit.contains::<MoveStep>();
            Some((
                Placed::new(position, body.shape()),
                body.layer(),
                owned,
                stands,
            ))
        })
        .collect();
    let walls = world.resource::<WallBook>();
    let on_walls = bodies
        .iter()
        .filter(|&&(placed, layer, owned, _)| owned && walls.overlaps(placed, layer))
        .count();
    let standing: Vec<_> = bodies.iter().filter(|body| body.3).collect();
    let mut pairs = 0;
    for (at, &&(placed, layer, owned, _)) in standing.iter().enumerate() {
        pairs += standing[at + 1..]
            .iter()
            .filter(|&&&(other, other_layer, other_owned, _)| {
                (owned || other_owned) && layer == other_layer && placed.overlaps(other)
            })
            .count();
    }
    on_walls + pairs
}
