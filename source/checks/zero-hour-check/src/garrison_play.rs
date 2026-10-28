use std::num::NonZeroU32;

use campfire_capabilities::Order;
use campfire_declarations::ById;
use campfire_harness::MatchUnits;
use campfire_interaction::EnterSpec;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_sim::StableId;
use campfire_units::{GroundPoint, Owner};
use campfire_values::UnitType;

use crate::failure::CheckFailure;
use crate::match_play::{BaseStarts, MatchPlay, PlayTools};
use crate::pins::{GarrisonSteps, GarrisonTicks};

/// America's units the play orders, and GLA's.
const RANGER: &str = "americainfantryranger";
const CRUSADER: &str = "americatankcrusader";
const REBEL: &str = "glainfantryrebel";

/// The garrison play on Tournament Desert (04-capabilities/interaction.md, A free building takes
/// its riders' side; 04-capabilities/combat.md, A container's modifier holds on its riders;
/// 04-capabilities/interaction.md, A container's fall hurts its riders): player 1, America, and
/// player 2, GLA, each from its base start, ordered nothing but the play's orders. Two Rebels
/// garrison the free building nearest the middle of the map that lets its riders out before it
/// dies, which turns GLA's; a Ranger that attacks its way to it takes the Rebels' fire before its
/// own reaches them, as their garrison stretches their reach; and America's Crusaders, attacking
/// their way there, shell the building until its Rebels leave it, at its share of life of leaving
/// or below, and it stands free again.
#[derive(Debug)]
pub(crate) struct GarrisonPlay;

/// The units the play orders, of their players' base starts, and the building.
#[derive(Debug, Clone, Copy)]
struct GarrisonUnits {
    building: StableId,
    ranger: StableId,
    crusaders: [StableId; 2],
    rebels: [StableId; 2],
}

impl GarrisonPlay {
    /// Plays the garrison in a match of `packages` at `rate` ticks a second: its steps, or none,
    /// with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<GarrisonSteps> {
        let played = BaseStarts::AMERICA_GLA.play(packages, rate, tools, Self::steps);
        played.map_err(|failure| failures.push(failure)).ok()
    }

    /// The play's steps in `play`, from its start's tick.
    fn steps(play: &mut MatchPlay<'_>) -> Result<GarrisonSteps, CheckFailure> {
        play.step();
        let units = GarrisonUnits::of(play)?;
        let building = units.building;

        let enter = |rebel| {
            let target = building;
            Order::one(rebel, OrdersDirective::Enter { target })
        };
        play.order_as(1, &units.rebels.map(enter));
        let garrisoned = play.step_until("the Rebels' garrison", |play| {
            units.rebels.iter().all(|&rebel| play.riding(rebel))
        })?;
        let turned = Self::owner(play, building) == Some(1);

        let ranger = units.ranger;
        let shots = |play: &MatchPlay<'_>| -> Vec<_> {
            play.times(ranger)
                .iter()
                .map(|time| time.ready_at)
                .collect()
        };
        let before = shots(play);
        let life = play.life(ranger);
        // The building stands in the fog of player 1's start, so its units walk at it and take
        // it as they see it.
        let at = MatchUnits::of(play.game()).position(building);
        let to = GroundPoint { x: at.x, z: at.z };
        play.order(&[Order::one(ranger, OrdersDirective::AttackMove { to })]);
        let outranged =
            play.step_until("the Ranger's first hit", |play| play.life(ranger) != life)?;
        let first = shots(play) == before;

        let shell = |crusader| Order::one(crusader, OrdersDirective::AttackMove { to });
        play.order(&units.crusaders.map(shell));
        let left = play.step_until("the Rebels' leave", |play| {
            units.rebels.iter().all(|&rebel| !play.riding(rebel))
        })?;
        let world = play.game();
        let unit_type = *MatchUnits::of(world)
            .unit(building)
            .get::<UnitType>()
            .expect("a unit of a type");
        let specs = world.resource::<ById<UnitType, EnterSpec>>();
        let full = specs.get(unit_type).expect("a garrison holds riders").full;
        let held = play
            .life_of(building)
            .is_some_and(|life| !life.is_empty() && life.current() <= full.of_num(life.max()));
        let freed = Self::owner(play, building).is_none();
        Ok(GarrisonSteps {
            ticks: GarrisonTicks {
                garrisoned,
                outranged,
                left,
            },
            turned,
            first,
            held,
            freed,
        })
    }

    /// The player slot that owns `unit`, from 0; none for a unit of no player.
    fn owner(play: &MatchPlay<'_>, unit: StableId) -> Option<u32> {
        let unit = MatchUnits::of(play.game()).unit(unit);
        unit.get::<Owner>().map(|owner| owner.player_slot().get())
    }
}

impl GarrisonUnits {
    /// The units of `play`'s base starts the play orders: the second Ranger, as the base start
    /// encloses the first in its buildings, the first two Crusaders and Rebels; and the free
    /// building nearest the middle of the starts whose riders leave it before it dies; a failure
    /// when a start or the map lacks one.
    fn of(play: &MatchPlay<'_>) -> Result<Self, CheckFailure> {
        let two = |unit, player_slot| -> Result<[StableId; 2], CheckFailure> {
            match play.owned(unit, player_slot)?[..] {
                [one, two, ..] => Ok([one, two]),
                _ => Err(CheckFailure::NoStartUnit { player_slot, unit }),
            }
        };
        let (one, two_at) = (play.start(1)?, play.start(2)?);
        let middle = Vec3::new(
            (one.x + two_at.x) / Num::int(2),
            Num::ZERO,
            (one.z + two_at.z) / Num::int(2),
        );
        let world = play.game();
        let specs = world.resource::<ById<UnitType, EnterSpec>>();
        let free = MatchUnits::of(world).all().filter(|(_, unit)| {
            let spec = unit
                .get::<UnitType>()
                .and_then(|&unit_type| specs.get(unit_type));
            unit.get::<Owner>().is_none()
                && spec.is_some_and(|spec| spec.capture && !spec.full.is_zero())
        });
        let building = free
            .map(|(id, _)| {
                let off = MatchUnits::of(world).position(id).ground_offset(middle);
                (off.length(), id)
            })
            .min()
            .map(|(_, id)| id)
            .ok_or(CheckFailure::NoGarrison)?;
        Ok(Self {
            building,
            ranger: two(RANGER, 0)?[1],
            crusaders: two(CRUSADER, 0)?,
            rebels: two(REBEL, 1)?,
        })
    }
}
