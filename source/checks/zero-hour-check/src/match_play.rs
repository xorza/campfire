use campfire_actions::TypeSlotsData;
use std::mem;
use std::num::NonZeroU32;

use campfire_bots::{
    BotBuffers, BotNames, BotScript, BotSink, HeldBotView, OrderScript, OrderUnsent, PacedBot,
    ScriptedOrder, ScriptedUnits, Think, ThinkError, ThinkingBot,
};
use campfire_capabilities::{Books, Directive, GroundLocation, Location, Match, NamedRefs, Order};
use campfire_common::{DeclaredName, Seat, Tick};
use campfire_declarations::MatchFacts;
use campfire_harness::{FixedMatch, FixedSession, MatchUnits};
use campfire_math::{Num, Vec3};
use campfire_mode::internals::MarkerOf;
use campfire_mode::{MatchEnd, Mode, OptionPicks, OptionValue, SeatPick};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_players::PlayersWorld;
use campfire_production::Site;
use campfire_protocol::{InputRules, SeatPlan, ServerInput};
use campfire_runner::SessionWorld;
use campfire_script::Budget;
use campfire_sim::MatchRate;
use campfire_sim::{Position, StableId};
use campfire_units::{BuildTarget, GroundPoint, Life, MapPoint, MoveStep, Units, UnitsWorld};
use campfire_values::{ResourceId, UnitType};

use crate::failure::CheckFailure;
use crate::pins::{BotMatchEnd, HZ};

/// America's command center, the building a skirmish starts a player with.
pub(crate) const CENTER: &str = "americacommandcenter";
/// America's dozer, the unit a skirmish starts a player with.
pub(crate) const DOZER: &str = "americavehicledozer";

/// China's dozer, the unit a skirmish starts China's player with.
pub(crate) const CHINA_DOZER: &str = "chinavehicledozer";

/// The money every player starts with, the most of Zero Hour's choices, which a play's host
/// picks (09-zero-hour.md, The import writes Zero Hour's money as an option).
pub(crate) const START_MONEY: &str = "start_money=50000";

/// Player 1's pick of China (09-zero-hour.md, Each player picks a faction).
pub(crate) const CHINA: &str = "0:faction=china";

/// The most ticks a play waits for a step, past which it never ends.
pub(crate) const STEP_LIMIT: u64 = HZ * 60 * 5;

/// The faction every seat of a play picks, America, whose start the plays read, in place of a
/// faction drawn (09-zero-hour.md, No pick is Random).
const AMERICA: &str = "faction=america";

/// A building player 1's dozer builds: its unit type, its build action, and its place,
/// `[x, z, angle]`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlannedBuilding {
    pub(crate) unit: &'static str,
    pub(crate) action: &'static str,
    pub(crate) at: [i64; 3],
}

/// Player 1's dozer that builds: its unit type and its stable id.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuildingDozer {
    pub(crate) unit: &'static str,
    pub(crate) id: StableId,
}

/// A match a play of the check drives a tick at a time, as player 1, the tick it runs next, the
/// bots it drives, the books a bot's think resolves its orders' names in, once one thinks, and
/// the first think that failed. A part of the imported mode a play reads and does not find is a
/// failure of the import.
#[derive(Debug)]
pub(crate) struct MatchPlay {
    fixed: FixedMatch,
    tick: Tick,
    bots: Vec<MatchBot>,
    books: Option<Books>,
    failed: Option<ThinkError>,
}

/// A bot a play drives as a server's bot driver does: its seat, what plays it, the view of the
/// match it reads, as the driver's does, and its buffers, the payloads it gives, its home, and the
/// farthest any of its units stood from there.
#[derive(Debug)]
struct MatchBot {
    seat: u32,
    plays: MatchBotPlays,
    view: HeldBotView,
    buffers: BotBuffers,
    due: DuePayloads,
    home: Vec3,
    farthest: Num,
}

/// What plays a play's bot: an order script, by its step; or a bot of the mode, at its pace, as
/// the server's driver plays it.
#[derive(Debug)]
enum MatchBotPlays {
    Orders(BotScript),
    Thinks(PacedBot),
}

/// The payloads a bot's step gives in a tick, which the play serves once the step, which reads
/// the match, is done; an order of no unit the server's driver drops goes unsent here too.
#[derive(Debug, Default)]
struct DuePayloads(Vec<Vec<u8>>);

impl BotSink for DuePayloads {
    fn send(&mut self, _: Seat, payload: &[u8]) {
        self.0.push(payload.to_vec());
    }

    fn unsent(&mut self, _: Seat, _: &Directive<NamedRefs>, _: OrderUnsent) {}
}

/// America for each of `seats` seats, as a host picks it.
pub(crate) fn america(seats: u32) -> Vec<SeatPick> {
    (0..seats)
        .map(|seat| {
            let pick = format!("{seat}:{AMERICA}");
            pick.parse().expect("a seat's pick of America reads")
        })
        .collect()
}

/// A play's match against America's bot: player 1's pick of its faction.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AgainstAmerica {
    player: &'static str,
}

impl AgainstAmerica {
    pub(crate) const CHINA: Self = Self { player: CHINA };
    pub(crate) const AMERICA: Self = Self {
        player: "0:faction=america",
    };
    pub(crate) const GLA: Self = Self {
        player: "0:faction=gla",
    };

    /// Plays `steps` in a match of `packages` at `rate` ticks a second, of player 1 against
    /// America's bot, player 1 starting with `START_MONEY`, and ends the match; what they
    /// measured, or their failure, or a failure when the mode takes no such pick.
    pub(crate) fn play<T>(
        self,
        packages: ModePackages,
        rate: NonZeroU32,
        steps: impl FnOnce(&mut MatchPlay, &ModePackages) -> Result<T, CheckFailure>,
    ) -> Result<T, CheckFailure> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = OptionPicks::named(&packages.data().options, &[start_money])
            .map_err(CheckFailure::Options)?;
        let plan = [SeatPlan::Player, SeatPlan::Bot];
        let player: SeatPick = self.player.parse().expect("the play's pick");
        let bot = america(2).pop().expect("the bot's pick of America");
        let session = FixedSession::planned(packages, InputRules::ROOMY, plan)
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&[player, bot]);
        let mut play = MatchPlay::new(session.start());
        let played = steps(&mut play, session.packages());
        let ended = play.end();
        let played = played?;
        ended?;
        Ok(played)
    }
}

impl MatchPlay {
    /// `fixed`, before its first tick.
    pub(crate) const fn new(fixed: FixedMatch) -> Self {
        Self {
            fixed,
            tick: Tick::new(0),
            bots: Vec::new(),
            books: None,
            failed: None,
        }
    }

    /// Drives player `seat` by `script` from the tick it runs next, as a server drives its bot,
    /// beside the bots it drives already, and keeps the farthest its units stand from `home`.
    pub(crate) fn drive_bot(&mut self, seat: u32, script: BotScript, home: Vec3) {
        self.drive(seat, MatchBotPlays::Orders(script), home);
    }

    /// Drives player `seat` by the bot `name` of `packages` from the tick it runs next, as a
    /// server drives a bot of its mode, beside the bots it drives already, and keeps the farthest
    /// its units stand from `home`; a failure when the mode names no such bot.
    pub(crate) fn drive_mode_bot(
        &mut self,
        packages: &ModePackages,
        seat: u32,
        name: &'static str,
        home: Vec3,
    ) -> Result<(), CheckFailure> {
        let declared = DeclaredName::new(name).expect("a bot's name");
        let bot = packages.bot(&declared).ok_or(CheckFailure::NoBot(name))?;
        let rate = *self.game().resource::<MatchRate>();
        let bot = PacedBot::new(ThinkingBot::new(bot), rate);
        self.books.get_or_insert_with(|| packages.name_books());
        self.drive(seat, MatchBotPlays::Thinks(bot), home);
        Ok(())
    }

    /// Drives player `seat` by `plays`, keeping the farthest its units stand from `home`.
    fn drive(&mut self, seat: u32, plays: MatchBotPlays, home: Vec3) {
        self.bots.push(MatchBot {
            seat,
            plays,
            view: HeldBotView::new(self.fixed.game_mut()),
            buffers: BotBuffers::default(),
            due: DuePayloads::default(),
            home,
            farthest: Num::ZERO,
        });
    }

    /// The point of the start waypoint `Player_<n>_Start`; a failure on a map with none.
    pub(crate) fn start(&self, n: u32) -> Result<Vec3, CheckFailure> {
        let name = format!("Player_{n}_Start");
        let of = MarkerOf {
            tag: "start",
            param: "name",
            value: &name,
        };
        Mode::marker_point(self.game(), of).ok_or(CheckFailure::NoStarts)
    }

    /// Where America's command center of player `seat` stands; a failure when the player has no
    /// one center.
    pub(crate) fn center(&self, seat: u32) -> Result<Vec3, CheckFailure> {
        let [center] = self.owned(CENTER, seat)?[..] else {
            return Err(CheckFailure::FightStart);
        };
        Ok(MatchUnits::of(self.game()).position(center))
    }

    /// Drives the bot, player 2, by one order in the tick it runs next, of `packages`: its dozer
    /// moves to the point `short` meters along −z of player 1's `unit`; the dozer, or a failure
    /// when the bot lacks its one dozer or the mode a name the order reads.
    pub(crate) fn drive_bot_dozer(
        &mut self,
        packages: &ModePackages,
        unit: StableId,
        short: Num,
    ) -> Result<StableId, CheckFailure> {
        let at = MatchUnits::of(self.game()).position(unit);
        let to = MapPoint::Ground([at.x, at.z - short]);
        let drive = ScriptedOrder {
            tick: self.tick,
            units: ScriptedUnits::Typed(DeclaredName::new(DOZER).expect("the dozer's name")),
            directive: Directive::Orders(OrdersDirective::Move {
                to: GroundLocation::At(Location::At(to)),
            }),
        };
        let script = OrderScript::of_orders([drive]).map_err(CheckFailure::BotOrders)?;
        let books = packages.name_books();
        let bot =
            BotScript::new(&script, &BotNames::new(&books)).map_err(CheckFailure::BotNames)?;
        let home = self.center(1)?;
        self.drive_bot(1, bot, home);
        let [dozer] = self.owned(DOZER, 1)?[..] else {
            return Err(CheckFailure::FightStart);
        };
        Ok(dozer)
    }

    /// Runs ticks until the match ends or its tick reaches `limit`: the tick it ended and its
    /// result, if it did, and the buildings, units that do not walk, each whole and alive, that
    /// players 1 and 2 hold then.
    pub(crate) fn fight(&mut self, limit: u64) -> BotMatchEnd {
        let end = loop {
            self.step();
            let world = self.game();
            let end = world
                .match_entity()
                .and_then(|entity| world.get::<MatchEnd>(entity).copied());
            if end.is_some() || self.tick.get() >= limit {
                break end;
            }
        };
        let standing = [0, 1].map(|seat| {
            MatchUnits::of(self.game())
                .all()
                .filter(|(_, unit)| {
                    MatchUnits::owned_by(unit, seat)
                        && !unit.contains::<MoveStep>()
                        && !unit.contains::<Site>()
                        && unit.get::<Life>().is_some_and(|life| !life.is_empty())
                })
                .count()
        });
        BotMatchEnd {
            ended: end.map(|end| end.tick().get()),
            result: end.map(MatchEnd::result),
            standing,
        }
    }

    /// The farthest a unit of the first bot stood from its home, on the ground plane, in meters;
    /// none with no bot.
    pub(crate) fn bot_farthest(&self) -> Option<Num> {
        self.bots.first().map(|bot| bot.farthest)
    }

    /// The tick it runs next: how many ticks it ran.
    pub(crate) const fn tick(&self) -> Tick {
        self.tick
    }

    pub(crate) const fn game(&self) -> &Match {
        self.fixed.game()
    }

    /// Logs each bot's orders its script has due, then runs the tick, and moves on to the next.
    pub(crate) fn step(&mut self) {
        let mut bots = mem::take(&mut self.bots);
        for bot in &mut bots {
            self.serve_bot(bot);
        }
        self.bots = bots;
        self.fixed.game_mut().run_tick_alone();
        self.tick = Tick::new(self.tick.get() + 1);
        for bot in &mut self.bots {
            let units = MatchUnits::of(self.fixed.game());
            let seat = bot.seat;
            for (_, unit) in units
                .all()
                .filter(|(_, unit)| MatchUnits::owned_by(unit, seat))
            {
                let Some(position) = unit.get::<Position>() else {
                    continue;
                };
                let distance = bot.home.ground_offset(position.get()).length();
                bot.farthest = bot.farthest.max(distance);
            }
        }
    }

    /// Logs what `bot` has due by the tick it runs next, as the server's bot driver gives it: an
    /// order script's mode inputs, then its orders, each for the bot's units it names; a bot's
    /// orders of its think, when it thinks, from a pool of the mode's `think` limit, as a tick's
    /// thinks share one. The first think that fails the play keeps.
    fn serve_bot(&mut self, bot: &mut MatchBot) {
        let MatchBot {
            seat,
            plays,
            view,
            buffers,
            due,
            ..
        } = bot;
        let seat = Seat::new(*seat);
        match plays {
            MatchBotPlays::Orders(script) => {
                let view = view.get(self.fixed.game());
                script.step(self.tick, seat, &view, buffers, due);
            }
            MatchBotPlays::Thinks(bot) => {
                let world = self.fixed.game_mut();
                let pools = world.resource::<MatchFacts>().role.pools();
                let pool = pools.expect("a check's match runs scripts").limits.think;
                let books = self
                    .books
                    .as_ref()
                    .expect("a bot of the mode keeps the books");
                let think = Think {
                    seat,
                    now: self.tick,
                    budget: &mut Budget::new(pool),
                    names: &BotNames::new(books),
                    view,
                    buffers,
                    sink: due,
                };
                if let Err(error) = bot.play(world, think) {
                    self.failed.get_or_insert(error);
                }
            }
        }
        for payload in due.0.drain(..) {
            let input = ServerInput::Bot {
                seat,
                payload: &payload,
            };
            self.fixed
                .serve(input)
                .expect("a bot's input within the session's limits");
        }
    }

    /// Sends player 1's `orders`, stamped for the tick it runs next.
    pub(crate) fn order(&mut self, orders: &[Order]) {
        self.fixed.send(0, self.tick, &Order::payload(orders));
    }

    /// The living units player `seat` owns of the type `name`, by stable id; a failure when the
    /// mode has no such type.
    pub(crate) fn owned(
        &self,
        name: &'static str,
        seat: u32,
    ) -> Result<Vec<StableId>, CheckFailure> {
        let world = self.fixed.game();
        let typed: UnitType = Units::type_named(world, name).ok_or(CheckFailure::NoType(name))?;
        let owned = MatchUnits::of(world)
            .all()
            .filter(|(_, unit)| {
                unit.get::<UnitType>() == Some(&typed)
                    && MatchUnits::owned_by(unit, seat)
                    && unit.get::<Life>().is_none_or(|life| !life.is_empty())
            })
            .map(|(id, _)| id)
            .collect();
        Ok(owned)
    }

    /// Orders `unit` to `directive` in the tick it runs next, then runs ticks until `done`; the
    /// ticks it took, the order's own among them, or a failure past `STEP_LIMIT`.
    pub(crate) fn until(
        &mut self,
        unit: StableId,
        directive: OrdersDirective,
        done: impl Fn(&Self) -> Result<bool, CheckFailure>,
    ) -> Result<u64, CheckFailure> {
        self.order(&[Order::one(unit, directive)]);
        for ticks in 1..=STEP_LIMIT {
            self.step();
            if done(self)? {
                return Ok(ticks);
            }
        }
        Err(CheckFailure::ProductionStep(directive))
    }

    /// Orders `builder` to place `building` with its build of `packages` in the tick it runs
    /// next, then runs ticks until the building stands whole, its site done; the ticks it took.
    pub(crate) fn build(
        &mut self,
        packages: &ModePackages,
        builder: BuildingDozer,
        building: PlannedBuilding,
    ) -> Result<u64, CheckFailure> {
        let [x, z, angle] = building.at.map(Num::int);
        let build = OrdersDirective::Build {
            slot: slot(packages, builder.unit, building.action)?,
            target: BuildTarget::Point {
                at: GroundPoint { x, z },
                angle,
            },
        };
        self.until(builder.id, build, |play| {
            let built = play.owned(building.unit, 0)?;
            let units = MatchUnits::of(play.game());
            Ok(built.len() == 1 && !units.unit(built[0]).contains::<Site>())
        })
    }

    /// The life of `unit`, none once it is gone.
    pub(crate) fn life(&self, unit: StableId) -> Option<Num> {
        let units = MatchUnits::of(self.game());
        let unit = units.all().find(|(id, _)| *id == unit)?.1;
        unit.get::<Life>().map(|life| life.current())
    }

    /// Player 1's amount of `money`.
    pub(crate) fn money(&self, money: ResourceId) -> i64 {
        self.game()
            .player_resources(Seat::new(0))
            .expect("player 1 holds resources")
            .amount(money)
    }

    /// Ends the match: it takes the log's events while it lives, as its harness checks them, so
    /// it ends before the check logs again; the first think of a bot that failed, as a failure.
    pub(crate) fn end(mut self) -> Result<(), CheckFailure> {
        self.fixed.end();
        self.failed
            .map_or(Ok(()), |error| Err(CheckFailure::BotThink(error)))
    }
}

/// The bot of the order script `text`, its names those of `packages`; a failure when it is no
/// script or names what the mode lacks.
pub(crate) fn bot_script(packages: &ModePackages, text: &str) -> Result<BotScript, CheckFailure> {
    let script = OrderScript::parse(text).map_err(CheckFailure::BotOrders)?;
    let books = packages.name_books();
    BotScript::new(&script, &BotNames::new(&books)).map_err(CheckFailure::BotNames)
}

/// The slot of `action` among the actions of the type `unit` of `packages` in its slot kind
/// `command`; a failure when the mode lacks the type or the action.
pub(crate) fn slot(
    packages: &ModePackages,
    unit: &'static str,
    action: &'static str,
) -> Result<u8, CheckFailure> {
    let command = DeclaredName::new("command").expect("a declared name");
    let at = packages
        .content()
        .units
        .get(unit)
        .and_then(|held| held.section::<TypeSlotsData>()?.0.get(&command))
        .and_then(|slots| slots.iter().position(|held| held.as_str() == action))
        .ok_or(CheckFailure::NoAction { unit, action })?;
    Ok(u8::try_from(at).expect("a unit's slots fit a u8"))
}
