use std::iter;
use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_harness::MatchUnits;
use campfire_math::Vec3;
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_sim::StableId;
use campfire_values::{TagSet, UnitTags};

use crate::failure::CheckFailure;
use crate::match_play::{self, BaseStarts, MatchPlay, PlayTools};
use crate::pins::{EntrySteps, EntryTicks};

/// China's units the play orders, and the bunker's research and tag.
const CRAWLER: &str = "chinavehicletroopcrawler";
const HELIX: &str = "chinavehiclehelix";
const OVERLORD: &str = "chinatankoverlord";
const BUNKER: &str = "research_upgrade_chinaoverlordbattlebunker";
const BUNKER_TAG: &str = "upgrade_chinaoverlordbattlebunker_created";

/// China's player slot in the play's match.
const CHINA: u32 = 1;

/// The entry play on Tournament Desert (entries-and-blocking.md, A walk may go into a unit;
/// entries-and-blocking.md, A walker passes through the unit it walks into): player 1, America,
/// and player 2, China, each from its base start, China ordered nothing but the play's orders. Its
/// two Troop Crawlers let their 16 Red Guards out, as its first Overlord buys its battle bunker;
/// the first crawler's 8 walk back into it, then the second's into a Helix, of 5 places, and the 3
/// it leaves into the Overlord's bunker once bought, each container standing where it stood while
/// its riders walk in but the Helix, which lands for them.
#[derive(Debug)]
pub(crate) struct EntryPlay;

/// The units the play orders, of China's base start.
#[derive(Debug, Clone, Copy)]
struct EntryUnits {
    crawlers: [StableId; 2],
    helix: StableId,
    overlord: StableId,
}

impl EntryPlay {
    /// Plays the entries in a match of `packages` at `rate` ticks a second: its steps, or none,
    /// with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<EntrySteps> {
        let played = match_play::slot(&packages, OVERLORD, BUNKER).and_then(|bunker| {
            BaseStarts::AMERICA_CHINA.play(packages, rate, tools, |play| Self::steps(play, bunker))
        });
        played.map_err(|failure| failures.push(failure)).ok()
    }

    /// The play's steps in `play`, from its start's tick, the bunker's research of the Overlord's
    /// slot `bunker`.
    fn steps(play: &mut MatchPlay<'_>, bunker: u8) -> Result<EntrySteps, CheckFailure> {
        play.step();
        let units = EntryUnits::of(play)?;
        let [first, second] = units.crawlers.map(|crawler| play.riders(crawler));
        let research = OrdersDirective::Slot {
            slot: bunker,
            target: ActionTarget::None,
        };
        let evacuate =
            (units.crawlers.iter()).map(|&crawler| Order::one(crawler, OrdersDirective::Evacuate));
        let orders: Vec<Order> = iter::once(Order::one(units.overlord, research))
            .chain(evacuate)
            .collect();
        play.order_as(CHINA, &orders);
        let out = play.step_until("the crawlers' evacuation", |play| {
            first
                .iter()
                .chain(&second)
                .all(|&guard| !play.riding(guard))
        })?;

        let at = |play: &MatchPlay<'_>, unit| MatchUnits::of(play.game()).position(unit);
        let crawler_at = at(play, units.crawlers[0]);
        play.order_as(
            CHINA,
            &Self::enter(&first, units.crawlers[0]).collect::<Vec<_>>(),
        );
        let crawler = play.step_until("the crawler's boarding", |play| {
            play.riders(units.crawlers[0]).len() == first.len()
        })?;
        let crawler_still = at(play, units.crawlers[0]) == crawler_at;
        play.order_as(
            CHINA,
            &Self::enter(&second, units.helix).collect::<Vec<_>>(),
        );
        let helix = play.step_until("the Helix's boarding", |play| {
            let walking = second
                .iter()
                .any(|&guard| play.destination(guard).is_some());
            !play.riders(units.helix).is_empty() && !walking
        })?;
        let left: Vec<StableId> = second
            .iter()
            .copied()
            .filter(|&guard| !play.riding(guard))
            .collect();

        play.step_until("the bunker's research", |play| {
            let units_of = MatchUnits::of(play.game());
            let tags = units_of
                .unit(units.overlord)
                .get::<UnitTags>()
                .map_or(TagSet::NONE, |tags| tags.tags);
            units_of
                .tag(BUNKER_TAG)
                .is_some_and(|tag| tags.contains(tag))
        })?;
        let overlord_at = at(play, units.overlord);
        let orders: Vec<Order> = Self::enter(&left, units.overlord).collect();
        play.order_as(CHINA, &orders);
        let bunker = play.step_until("the bunker's boarding", |play| {
            left.iter().all(|&guard| play.riding(guard))
        })?;
        let still = |play: &MatchPlay<'_>, unit, was: Vec3| at(play, unit) == was;
        Ok(EntrySteps {
            ticks: EntryTicks {
                out,
                crawler,
                helix,
                bunker,
            },
            riders: [units.crawlers[0], units.helix, units.overlord]
                .map(|container| play.riders(container).len()),
            still: [crawler_still, still(play, units.overlord, overlord_at)],
        })
    }

    /// The orders of each of `riders` into `target`.
    fn enter(riders: &[StableId], target: StableId) -> impl Iterator<Item = Order> + '_ {
        let enter = OrdersDirective::Enter { target };
        riders.iter().map(move |&rider| Order::one(rider, enter))
    }
}

impl EntryUnits {
    /// The units of China's base start the play orders: its first two Troop Crawlers, its first
    /// Helix and its first Overlord; a failure when its start lacks one.
    fn of(play: &MatchPlay<'_>) -> Result<Self, CheckFailure> {
        let first = |unit| -> Result<StableId, CheckFailure> {
            let owned = play.owned(unit, CHINA)?;
            owned.first().copied().ok_or(CheckFailure::NoStartUnit {
                player_slot: CHINA,
                unit,
            })
        };
        let crawlers = match play.owned(CRAWLER, CHINA)?[..] {
            [one, two, ..] => [one, two],
            _ => {
                return Err(CheckFailure::NoStartUnit {
                    player_slot: CHINA,
                    unit: CRAWLER,
                });
            }
        };
        Ok(Self {
            crawlers,
            helix: first(HELIX)?,
            overlord: first(OVERLORD)?,
        })
    }
}
