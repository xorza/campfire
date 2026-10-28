use std::num::NonZeroU32;

use campfire_actions::TargetSource;
use campfire_capabilities::Order;
use campfire_common::Tick;
use campfire_harness::MatchUnits;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_sim::StableId;
use campfire_units::GroundPoint;

use crate::failure::CheckFailure;
use crate::match_play::{self, BaseStarts, MatchPlay, PlayTools, PlayWalk, STEP_LIMIT};
use crate::pins::{CombatSteps, CombatTicks};

/// America's units the play orders, and GLA's.
const COMANCHE: &str = "americavehiclecomanche";
const CRUSADER: &str = "americatankcrusader";
const REBEL: &str = "glainfantryrebel";
const SCORPION: &str = "glatankscorpion";
const BUGGY: &str = "glavehiclerocketbuggy";

/// The Comanche's weapons: its cannon, which prefers infantry, and its missiles, the stronger
/// against a tank.
const CANNON: &str = "weapon_comanche20mmcannonweapon";
const MISSILES: &str = "weapon_comancheantitankmissileweapon";

/// Where the play's units go, `[x, z]` meters from the middle of the two starts, in the open
/// ground between the map's ridges, clear of its buildings. The Comanche waits 350 m from its targets, past its weapons'
/// 200 m, so its orders alone start its fights. The buggy stops 215 m from the first Crusader,
/// their bodies 181 m apart: past the Crusader's gun of 150 m, within the buggy's rockets of
/// 300 m and the mode's retaliation distance of 200 m. The second Crusader stands 60 m from the
/// first, within the mode's radius of 120 m, and 223 m from the buggy, past its own gun. The
/// driving Crusader passes 100 m from the second Rebel, within its gun's reach, which stands
/// 340 m from the other Crusader.
const REBEL_AT: [i64; 2] = [0, 0];
const SCORPION_AT: [i64; 2] = [100, 0];
const COMANCHE_AT: [i64; 2] = [50, 350];
const CRUSADERS_AT: [[i64; 2]; 2] = [[-400, 0], [-400, 60]];
const BUGGY_AT: [i64; 2] = [-185, 0];
const DRIVEN_REBEL_AT: [i64; 2] = [-100, -100];
const DRIVE_TO: [i64; 2] = [150, 0];

/// The combat play on Tournament Desert (combat-response.md, Combat chooses the weapon;
/// combat-response.md, A turret has its own target; combat-response.md, A damaged unit calls its
/// idle allies; combat-response.md, A play walks its units out of the base starts): player 1,
/// America, and player 2, GLA, each from its base start, ordered nothing but the play's orders,
/// which walk their units out to the middle of the map. America's Comanche, ordered at a Rebel
/// and then at a Scorpion, fires its cannon at the one and its missiles at the other; a rocket
/// buggy fires on a standing Crusader from past the Crusader's reach, and the Crusader and its
/// idle neighbour chase it and walk back to their places; and a Crusader driven past a Rebel
/// fires on it from its turret and drives on to its goal.
#[derive(Debug)]
pub(crate) struct CombatPlay;

/// The units the play orders, of their players' base starts.
#[derive(Debug, Clone, Copy)]
struct CombatUnits {
    comanche: StableId,
    crusaders: [StableId; 2],
    rebels: [StableId; 2],
    scorpion: StableId,
    buggy: StableId,
}

/// An attack's first shot: the ticks from its order, the order's own among them, and the slot of
/// the weapon its unit fired.
#[derive(Debug, Clone, Copy)]
struct FirstShot {
    ticks: u64,
    weapon: u8,
}

/// A Crusader's walk back from its retaliation: the ticks from the hit that called it until it
/// stood with no target and no destination, and how far it stood then from where it was called.
#[derive(Debug, Clone, Copy)]
struct WalkBack {
    ticks: u64,
    stray: Num,
}

impl CombatPlay {
    /// The slots of the Comanche's cannon and missiles in `packages`; a failure when the mode
    /// lacks one.
    pub(crate) fn weapons(packages: &ModePackages) -> Result<[u8; 2], CheckFailure> {
        let weapon = |action| match_play::slot_in(packages, COMANCHE, "weapon", action);
        Ok([weapon(CANNON)?, weapon(MISSILES)?])
    }

    /// Plays the combat in a match of `packages` at `rate` ticks a second: its steps, or none,
    /// with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<CombatSteps> {
        let played = BaseStarts::AMERICA_GLA.play(packages, rate, tools, Self::steps);
        played.map_err(|failure| failures.push(failure)).ok()
    }

    /// The play's steps in `play`, from its start's tick.
    fn steps(play: &mut MatchPlay<'_>) -> Result<CombatSteps, CheckFailure> {
        play.step();
        let units = CombatUnits::of(play)?;
        let (one, two) = (play.start(1)?, play.start(2)?);
        let at = |[x, z]: [i64; 2]| GroundPoint {
            x: (one.x + two.x) / Num::int(2) + Num::int(x),
            z: (one.z + two.z) / Num::int(2) + Num::int(z),
        };
        let walk = |player_slot, unit, to| PlayWalk {
            player_slot,
            unit,
            to: at(to),
        };

        play.walk(&[
            walk(1, units.rebels[0], REBEL_AT),
            walk(1, units.scorpion, SCORPION_AT),
            walk(0, units.comanche, COMANCHE_AT),
        ])?;
        let cannon = Self::first_shot(play, units.comanche, units.rebels[0])?;
        play.step_until("the Rebel's death", |play| play.dead(units.rebels[0]))?;
        let missiles = Self::first_shot(play, units.comanche, units.scorpion)?;
        play.step_until("the Scorpion's death", |play| play.dead(units.scorpion))?;
        let home = GroundPoint { x: one.x, z: one.z };
        play.order(&[Order::one(
            units.comanche,
            OrdersDirective::Move { to: home },
        )]);

        play.walk(&[
            walk(0, units.crusaders[0], CRUSADERS_AT[0]),
            walk(0, units.crusaders[1], CRUSADERS_AT[1]),
        ])?;
        play.walk(&[walk(1, units.buggy, BUGGY_AT)])?;
        let rockets = Self::first_hit(play, units.buggy, units.crusaders[0])?;
        // The call comes in the tick of the hit, once its damage applied.
        let called = units.crusaders.map(|crusader| {
            let progress = play.progress(crusader);
            match progress.target_source() {
                Some(TargetSource::Retaliation { from, .. })
                    if progress.attack_target() == Some(units.buggy) =>
                {
                    Some(from)
                }
                _ => None,
            }
        });
        let [Some(first), Some(second)] = called else {
            return Err(CheckFailure::NoStep("the Crusaders' call"));
        };
        let mut back = [None; 2];
        for ticks in 1..=STEP_LIMIT {
            play.step();
            for ((back, crusader), from) in
                back.iter_mut().zip(units.crusaders).zip([first, second])
            {
                let progress = play.progress(crusader);
                if back.is_none()
                    && progress.attack_target().is_none()
                    && play.destination(crusader).is_none()
                {
                    let stray = MatchUnits::of(play.game())
                        .position(crusader)
                        .ground_offset(from)
                        .length();
                    *back = Some(WalkBack { ticks, stray });
                }
            }
            if back.iter().all(Option::is_some) {
                break;
            }
        }
        let [Some(first_back), Some(second_back)] = back else {
            return Err(CheckFailure::NoStep("the Crusaders' walk back"));
        };

        play.walk(&[walk(1, units.rebels[1], DRIVEN_REBEL_AT)])?;
        let driver = units.crusaders[0];
        let goal = at(DRIVE_TO);
        play.order(&[Order::one(driver, OrdersDirective::Move { to: goal })]);
        let life = play.life(units.rebels[1]);
        let turret = play.step_until("the driven Crusader's hit", |play| {
            play.life(units.rebels[1]) != life
        })?;
        let progress = play.progress(driver);
        let drove = play.destination(driver).is_some()
            && progress.attack_target().is_none()
            && progress.turret_target() == Some(units.rebels[1]);
        let arrived = turret
            + play.step_until("the driven Crusader's goal", |play| {
                play.destination(driver).is_none()
            })?;
        let end = MatchUnits::of(play.game()).position(driver);
        let left = end
            .ground_offset(Vec3::new(goal.x, Num::ZERO, goal.z))
            .length();
        Ok(CombatSteps {
            ticks: CombatTicks {
                cannon: cannon.ticks,
                missiles: missiles.ticks,
                rockets,
                back: [first_back.ticks, second_back.ticks],
                turret,
                arrived,
            },
            weapons: [cannon.weapon, missiles.weapon],
            strays: [first_back.stray, second_back.stray],
            drove,
            left,
        })
    }

    /// Orders player 1's `unit` to attack `target` in the tick `play` runs next, then runs ticks
    /// until a weapon of it fires, as its slot's time to be ready again moves.
    fn first_shot(
        play: &mut MatchPlay<'_>,
        unit: StableId,
        target: StableId,
    ) -> Result<FirstShot, CheckFailure> {
        let before: Vec<Tick> = play.times(unit).iter().map(|time| time.ready_at).collect();
        let fired = |play: &MatchPlay<'_>| {
            let times = play.times(unit).iter();
            let at = times
                .zip(&before)
                .position(|(time, &was)| time.ready_at != was)?;
            Some(u8::try_from(at).expect("a unit's slots fit a u8"))
        };
        play.order(&[Order::one(unit, OrdersDirective::Attack { target })]);
        let ticks = play.step_until("an attack's first shot", |play| fired(play).is_some())?;
        Ok(FirstShot {
            ticks,
            weapon: fired(play).expect("a weapon fired"),
        })
    }

    /// Orders player 2's `unit` to attack `target` in the tick `play` runs next, then runs ticks
    /// until the target loses life; the ticks it took, the order's own among them.
    fn first_hit(
        play: &mut MatchPlay<'_>,
        unit: StableId,
        target: StableId,
    ) -> Result<u64, CheckFailure> {
        let life = play.life(target);
        play.order_as(1, &[Order::one(unit, OrdersDirective::Attack { target })]);
        play.step_until("an attack's first hit", |play| play.life(target) != life)
    }
}

impl CombatUnits {
    /// The units of `play`'s base starts the play orders: the first of each type, and the first
    /// two Crusaders and Rebels; a failure when a start lacks one.
    fn of(play: &MatchPlay<'_>) -> Result<Self, CheckFailure> {
        let first = |unit, player_slot| -> Result<StableId, CheckFailure> {
            let owned = play.owned(unit, player_slot)?;
            owned
                .first()
                .copied()
                .ok_or(CheckFailure::NoStartUnit { player_slot, unit })
        };
        let two = |unit, player_slot| -> Result<[StableId; 2], CheckFailure> {
            match play.owned(unit, player_slot)?[..] {
                [one, two, ..] => Ok([one, two]),
                _ => Err(CheckFailure::NoStartUnit { player_slot, unit }),
            }
        };
        Ok(Self {
            comanche: first(COMANCHE, 0)?,
            crusaders: two(CRUSADER, 0)?,
            rebels: two(REBEL, 1)?,
            scorpion: first(SCORPION, 1)?,
            buggy: first(BUGGY, 1)?,
        })
    }
}
