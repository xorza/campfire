use campfire_actions::{SlotKinds, TypeSlotsData};
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_bots::{
    BotNames, BotScript, BotSink, OrderScript, OrderUnsent, PacedBot, PlayerSlotPlay,
    ScriptedOrder, ScriptedUnits, ThinkError, ThinkingBot, TickBot, TickBots,
};
use campfire_capabilities::{Books, Directive, Location, Match, NamedRefs, Order};
use campfire_common::{DeclaredName, PlayerSlot, Tick};
use campfire_declarations::MatchFacts;
use campfire_harness::{FixedMatch, FixedSession, MatchUnits, ProbeFile, ReplayRecord};
use campfire_math::{Num, Vec3};
use campfire_mode::internals::MarkerOf;
use campfire_mode::{MatchEnd, Mode, OptionPicks, OptionValue, PlayerSlotPick};
use campfire_navigation::StalledWalkers;
use campfire_orders::{DroppedOrders, OrdersDirective};
use campfire_package::ModePackages;
use campfire_players::PlayersWorld;
use campfire_production::Site;
use campfire_protocol::PlayerSlotPlan;
use campfire_runner::SessionWorld;
use campfire_sim::MatchRate;
use campfire_sim::{Position, StableId};
use campfire_units::{BuildTarget, GroundPoint, Life, MapPoint, MoveStep, Units, UnitsWorld};
use campfire_values::{ResourceId, UnitType};
use campfire_vision::{CellBits, ExploredCells, SeenCells};
use tracing::info;

use crate::failure::CheckFailure;
use crate::pins::{BotDrops, BotMatchEnd, HZ, SightSent};

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
/// bots it drives as a server's bot driver drives them, by the one rule of a tick's bots, with
/// each bot's home and the farthest any of its units stood from there; the books a bot's think
/// resolves its orders' names in; and what the bots gave in the tick, with the first think that
/// failed. A part of the imported mode a play reads and does not find is a failure of the import.
#[derive(Debug)]
pub(crate) struct MatchPlay<'p> {
    fixed: FixedMatch,
    bots: TickBots,
    homes: Vec<BotHome>,
    /// The most operations one think of its bots ran so far.
    think_ops: u64,
    /// The times a walker stood stuck 1 s so far, as navigation's report of each tick counts
    /// them.
    stalls: u64,
    /// What the server sent players 1 and 2 of their teams' cells so far, and the cells each
    /// held after the last tick, those it sees and those it explored.
    sight: SightSent,
    cells: [[CellBits; 2]; 2],
    books: Arc<Books>,
    due: DuePayloads,
    /// The record of each tick, when the play keeps one: its copy checked, and its log replayed
    /// to the same hashes as it ends, so a determinism bug names its tick (09-zero-hour.md, A
    /// play keeps a record on request).
    record: Option<ReplayRecord<'p>>,
}

/// What a play watches its match with: the probe it writes, if any, and whether it keeps a
/// record.
#[derive(Debug)]
pub(crate) struct PlayTools {
    pub(crate) probe: Option<ProbeFile>,
    pub(crate) record: PlayRecord,
}

/// Whether a play keeps a record of its match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayRecord {
    None,
    Kept,
}

/// The home of a play's bot, and the farthest any of its units stood from there.
#[derive(Debug)]
struct BotHome {
    player_slot: PlayerSlot,
    home: Vec3,
    farthest: Num,
}

/// The payloads the bots give in a tick, each with its player slot, which the play serves once
/// they, which read the match, are done; an order of no unit the server's driver drops goes unsent
/// here too; the orders of each player slot that came to nothing so far, by its index; and the
/// first think that failed, which fails the play; and the most bytes the session lets a payload
/// hold.
#[derive(Debug)]
struct DuePayloads {
    max_payload_len: u32,
    payloads: Vec<(PlayerSlot, Vec<u8>)>,
    drops: Vec<BotDrops>,
    failed: Option<ThinkError>,
}

impl DuePayloads {
    /// The payloads of a play of `fixed`, none due yet.
    fn of(fixed: &FixedMatch) -> Self {
        let terms = &fixed.game().session().log().header().terms;
        Self {
            max_payload_len: terms.inputs.max_payload_len,
            payloads: Vec::new(),
            drops: Vec::new(),
            failed: None,
        }
    }

    /// The drops of `player_slot` so far.
    fn drops_of(&mut self, player_slot: PlayerSlot) -> &mut BotDrops {
        let index = player_slot.index();
        if self.drops.len() <= index {
            self.drops.resize(index + 1, BotDrops::default());
        }
        &mut self.drops[index]
    }
}

impl BotSink for DuePayloads {
    fn max_payload_len(&self) -> u32 {
        self.max_payload_len
    }

    fn send(&mut self, player_slot: PlayerSlot, payload: &[u8]) {
        self.payloads.push((player_slot, payload.to_vec()));
    }

    fn unsent(&mut self, player_slot: PlayerSlot, _: &Directive<NamedRefs>, cause: OrderUnsent) {
        self.drops_of(player_slot).unsent.count(cause);
    }

    fn failed(&mut self, _: PlayerSlot, error: ThinkError) {
        self.failed.get_or_insert(error);
    }
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
        tools: PlayTools,
        steps: impl FnOnce(&mut MatchPlay<'_>, &ModePackages) -> Result<T, CheckFailure>,
    ) -> Result<T, CheckFailure> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = OptionPicks::named(&packages.data().options, &[start_money])
            .map_err(CheckFailure::Options)?;
        let plan = [PlayerSlotPlan::Player, PlayerSlotPlan::Bot];
        let player: PlayerSlotPick = self.player.parse().expect("the play's pick");
        let bot: PlayerSlotPick = "1:faction=america".parse().expect("the bot's pick");
        let session = FixedSession::planned(packages, plan)
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&[player, bot]);
        let mut play = MatchPlay::new(session.start(), session.packages(), tools);
        let played = steps(&mut play, session.packages());
        let ended = play.end();
        let played = played?;
        ended?;
        Ok(played)
    }
}

impl<'p> MatchPlay<'p> {
    /// `fixed`, a match of `packages`, before its first tick, played with `tools`.
    pub(crate) fn new(mut fixed: FixedMatch, packages: &'p ModePackages, tools: PlayTools) -> Self {
        let world = fixed.game_mut();
        let setup = world.resource::<MatchFacts>().role.budgets();
        let think_limit = setup.expect("a check's match runs scripts").limits.think;
        let bots = TickBots::new(world, think_limit);
        let books = Arc::clone(packages.load_books());
        let PlayTools { probe, record } = tools;
        if let Some(probe) = probe {
            fixed.probe(probe);
        }
        let record = match record {
            PlayRecord::None => None,
            PlayRecord::Kept => Some(ReplayRecord::of(fixed.game_mut(), packages)),
        };
        let due = DuePayloads::of(&fixed);
        Self {
            fixed,
            bots,
            homes: Vec::new(),
            think_ops: 0,
            stalls: 0,
            sight: SightSent::default(),
            cells: Default::default(),
            books,
            due,
            record,
        }
    }

    /// Drives player `player_slot` by `script` from the tick it runs next, as a server drives its
    /// bot, beside the bots it drives already, and keeps the farthest its units stand from `home`.
    pub(crate) fn drive_bot(&mut self, player_slot: u32, script: BotScript, home: Vec3) {
        let play = PlayerSlotPlay::orders(script, Tick::new(0), self.fixed.next_tick());
        self.drive(player_slot, play, home);
    }

    /// Drives player `player_slot` by the bot `name` of `packages` from the tick it runs next, as a
    /// server drives a bot of its mode, beside the bots it drives already, and keeps the farthest
    /// its units stand from `home`; a failure when the mode names no such bot.
    pub(crate) fn drive_mode_bot(
        &mut self,
        packages: &ModePackages,
        player_slot: u32,
        name: &'static str,
        home: Vec3,
    ) -> Result<(), CheckFailure> {
        let declared = DeclaredName::new(name).expect("a bot's name");
        let bot = packages.bot(&declared).ok_or(CheckFailure::NoBot(name))?;
        let rate = *self.game().resource::<MatchRate>();
        let bot = PacedBot::new(ThinkingBot::new(bot), rate);
        self.drive(player_slot, PlayerSlotPlay::Thinks(bot), home);
        Ok(())
    }

    /// Drives player `player_slot` by `play`, keeping the farthest its units stand from `home`.
    fn drive(&mut self, player_slot: u32, play: PlayerSlotPlay, home: Vec3) {
        let player_slot = PlayerSlot::new(player_slot);
        self.bots.push(TickBot { player_slot, play });
        self.homes.push(BotHome {
            player_slot,
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

    /// Where America's command center of player `player_slot` stands; a failure when the player has
    /// no one center.
    pub(crate) fn center(&self, player_slot: u32) -> Result<Vec3, CheckFailure> {
        let [center] = self.owned(CENTER, player_slot)?[..] else {
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
            tick: self.fixed.next_tick(),
            units: ScriptedUnits::Typed(DeclaredName::new(DOZER).expect("the dozer's name")),
            directive: Directive::Orders(OrdersDirective::Move {
                to: Location::At(to),
            }),
        };
        let script = OrderScript::of_orders([drive]).map_err(CheckFailure::BotOrders)?;
        let books = packages.load_books();
        let bot = BotScript::new(&script, &BotNames::new(books)).map_err(CheckFailure::BotNames)?;
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
            if end.is_some() || self.fixed.next_tick().get() >= limit {
                break end;
            }
        };
        let standing = [0, 1].map(|player_slot| {
            MatchUnits::of(self.game())
                .all()
                .filter(|(_, unit)| {
                    MatchUnits::owned_by(unit, player_slot)
                        && !unit.contains::<MoveStep>()
                        && !unit.contains::<Site>()
                        && unit.get::<Life>().is_some_and(|life| !life.is_empty())
                })
                .count()
        });
        let drops = [0, 1].map(|index| self.due.drops.get(index).copied().unwrap_or_default());
        BotMatchEnd {
            ended: end.map(|end| end.tick().get()),
            result: end.map(MatchEnd::result),
            standing,
            think_ops: self.think_ops,
            stalls: self.stalls,
            drops,
            sight: self.sight,
        }
    }

    /// The farthest a unit of the first bot stood from its home, on the ground plane, in meters;
    /// none with no bot.
    pub(crate) fn bot_farthest(&self) -> Option<Num> {
        self.homes.first().map(|home| home.farthest)
    }

    /// The tick it runs next: how many ticks it ran.
    pub(crate) fn tick(&self) -> Tick {
        self.fixed.next_tick()
    }

    pub(crate) const fn game(&self) -> &Match {
        self.fixed.game()
    }

    /// Runs the match's next tick as a server runs it with its bots: their think, the serve of
    /// what they gave, the tick, and then what the play notes of it.
    pub(crate) fn step(&mut self) {
        let tick = self.fixed.next_tick();
        self.think();
        self.serve();
        self.run_tick();
        self.observe(tick);
    }

    /// Counts each change of the cells players 1 and 2 hold, those their teams see and those they
    /// explored, which the server sends them whole.
    fn count_sight(&mut self) {
        let world = self.fixed.game();
        for (at, held) in self.cells.iter_mut().enumerate() {
            let player_slot = PlayerSlot::new(u32::try_from(at).expect("two players"));
            let Some(player) = world.player_entity(player_slot) else {
                continue;
            };
            let seen = world.get::<SeenCells>(player).map(SeenCells::cells);
            let explored = world.get::<ExploredCells>(player).map(ExploredCells::cells);
            let counts = [&mut self.sight.seen[at], &mut self.sight.explored[at]];
            for ((cells, count), was) in [seen, explored].into_iter().zip(counts).zip(held) {
                if let Some(cells) = cells.filter(|&cells| cells != was) {
                    *count += 1;
                    was.clone_from(cells);
                    self.sight.words = cells.words().len();
                }
            }
        }
    }

    /// The bots' think for the tick the match runs next, as a server's bot driver runs it, which
    /// leaves their payloads due.
    fn think(&mut self) {
        let names = BotNames::new(&self.books);
        let tick = self.fixed.next_tick();
        let world = self.fixed.game_mut();
        let thinks = self.bots.play(world, tick, &names, &mut self.due);
        self.think_ops = self.think_ops.max(thinks.most);
    }

    /// Serves the bots' due payloads to the match, counting those it refuses.
    fn serve(&mut self) {
        for at in 0..self.due.payloads.len() {
            let (player_slot, payload) = &self.due.payloads[at];
            let player_slot = *player_slot;
            if let Err(cause) = self.fixed.serve_bot(player_slot, payload) {
                self.due.drops_of(player_slot).refused.count(cause);
            }
        }
        self.due.payloads.clear();
    }

    /// Runs the match's tick, as a server runs it, counting the inputs it left unread.
    fn run_tick(&mut self) {
        for unread in self.fixed.step_unread() {
            self.due.drops_of(unread.player_slot).unread += 1;
        }
    }

    /// Notes what the match's tick `tick` did that the play checks: its record, the orders it
    /// dropped, its stalls, and how far each bot's units stood from its home.
    fn observe(&mut self, tick: Tick) {
        if let Some(record) = &mut self.record {
            record.tick(self.fixed.game_mut());
        }
        // A tick that checked no orders, as a match that ended runs none, leaves an earlier
        // tick's drops.
        let dropped = self.fixed.game().get_resource::<DroppedOrders>();
        if let Some(dropped) = dropped.filter(|dropped| dropped.tick() == tick) {
            for drop in dropped {
                let drops = self.due.drops_of(drop.player_slot);
                drops.dropped.count(drop.cause);
            }
            for drop in dropped.untaken() {
                self.due.drops_of(drop.player_slot).untaken += 1;
            }
        }
        self.count_sight();
        let units = MatchUnits::of(self.fixed.game());
        let stalled = self.fixed.game().get_resource::<StalledWalkers>();
        let stalls = stalled.map_or(0, |stalled| stalled.get());
        self.stalls += u64::try_from(stalls).expect("a count of units fits u64");
        for BotHome {
            player_slot,
            home,
            farthest,
        } in &mut self.homes
        {
            for (_, unit) in units
                .all()
                .filter(|(_, unit)| MatchUnits::owned_by(unit, player_slot.get()))
            {
                let Some(position) = unit.get::<Position>() else {
                    continue;
                };
                *farthest = (*farthest).max(home.ground_offset(position.get()).length());
            }
        }
    }

    /// Sends player 1's `orders`, stamped for the tick it runs next.
    pub(crate) fn order(&mut self, orders: &[Order]) {
        self.fixed
            .send(0, self.fixed.next_tick(), &Order::payload(orders));
    }

    /// The living units player `player_slot` owns of the type `name`, by stable id; a failure when
    /// the mode has no such type.
    pub(crate) fn owned(
        &self,
        name: &'static str,
        player_slot: u32,
    ) -> Result<Vec<StableId>, CheckFailure> {
        let world = self.fixed.game();
        let typed: UnitType = Units::type_named(world, name).ok_or(CheckFailure::NoType(name))?;
        let owned = MatchUnits::of(world).owned(typed, player_slot);
        let living =
            owned.filter(|(_, unit)| unit.get::<Life>().is_none_or(|life| !life.is_empty()));
        Ok(living.map(|(id, _)| id).collect())
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
        MatchUnits::of(self.game()).amount(0, money)
    }

    /// Ends the match: it takes the log's events while it lives, as its harness checks them, so
    /// it ends before the check logs again; then writes its probe's file: the first think of a
    /// bot that failed, or why the file did not write, as a failure.
    pub(crate) fn end(mut self) -> Result<(), CheckFailure> {
        self.fixed.end();
        if let Some(record) = &self.record {
            record.assert_segments_replay(self.fixed.game().session().log(), &[]);
        }
        if let Some(error) = self.due.failed {
            return Err(CheckFailure::BotThink(error));
        }
        let Some(probe) = self.fixed.take_probe() else {
            return Ok(());
        };
        probe.write().map_err(CheckFailure::ProbeWrite)?;
        info!(file = %probe.file.display(), "the play's probe");
        Ok(())
    }
}

/// The bot of the order script `text`, its names those of `packages`; a failure when it is no
/// script or names what the mode lacks.
pub(crate) fn bot_script(packages: &ModePackages, text: &str) -> Result<BotScript, CheckFailure> {
    let script = OrderScript::parse(text).map_err(CheckFailure::BotOrders)?;
    let books = packages.load_books();
    BotScript::new(&script, &BotNames::new(books)).map_err(CheckFailure::BotNames)
}

/// The slot of `action`, of the type `unit` of `packages`, in its slot kind `command`: its place
/// among that kind's actions past the actions of every kind before it, as a unit holds its kinds
/// in the mode's order (04-capabilities/actions.md, Slot kinds in the mode's order); a failure
/// when the mode lacks the type or the action.
pub(crate) fn slot(
    packages: &ModePackages,
    unit: &'static str,
    action: &'static str,
) -> Result<u8, CheckFailure> {
    let missing = CheckFailure::NoAction { unit, action };
    let command = DeclaredName::new("command").expect("a declared name");
    let held = packages.content().units.get(unit);
    let slots = held.and_then(|held| held.section::<TypeSlotsData>());
    let Some(slots) = slots else {
        return Err(missing);
    };
    let kinds = packages.data().section_or_default::<SlotKinds>();
    let mut before = 0;
    for kind in kinds.kinds() {
        let actions = slots.0.get(&kind.name).map_or(&[][..], Vec::as_slice);
        if kind.name == command {
            let at = actions.iter().position(|held| held.as_str() == action);
            let at = at.ok_or(missing)?;
            return Ok(u8::try_from(before + at).expect("a unit's slots fit a u8"));
        }
        before += actions.len();
    }
    Err(missing)
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::time::{Duration, Instant};

    use campfire_capabilities::Match;

    use crate::match_play::MatchPlay;

    /// What a tick of a play took: its bots' think, and the match's tick as a server runs it.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct TickTimes {
        pub(crate) think: Duration,
        pub(crate) tick: Duration,
    }

    impl MatchPlay<'_> {
        /// Runs the match's next tick as `step` does, past what the play notes of it, timing its
        /// bots' think and its tick.
        pub(crate) fn timed_tick(&mut self) -> TickTimes {
            let start = Instant::now();
            self.think();
            let think = start.elapsed();
            self.serve();
            let start = Instant::now();
            self.run_tick();
            TickTimes {
                think,
                tick: start.elapsed(),
            }
        }

        /// The match it plays, to change outside its ticks, as a bench installs its probes.
        pub(crate) const fn game_mut(&mut self) -> &mut Match {
            self.fixed.game_mut()
        }
    }
}
