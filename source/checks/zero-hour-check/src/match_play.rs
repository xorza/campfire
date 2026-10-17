use campfire_bots::{BotBuffers, BotScript, BotSink, UnitsMissing};
use campfire_capabilities::{Action, Match, Order};
use campfire_common::{PlayerSlot, Tick};
use campfire_harness::{FixedMatch, MatchUnits};
use campfire_math::{Num, Vec3};
use campfire_protocol::ServerInput;
use campfire_runner::SessionWorld;
use campfire_sim::{Position, StableId};
use campfire_units::{HeldPlayerUnits, Life, Units};
use campfire_values::UnitType;

use crate::failure::Failure;

/// America's command center, the building a skirmish starts a player with.
pub(crate) const CENTER: &str = "americacommandcenter";
/// America's dozer, the unit a skirmish starts a player with.
pub(crate) const DOZER: &str = "americavehicledozer";

/// A match a play of the check drives a tick at a time, as player 1, the tick it runs next, and
/// the bot it drives, if one. A part of the imported mode a play reads and does not find is a
/// failure of the import.
#[derive(Debug)]
pub(crate) struct MatchPlay {
    fixed: FixedMatch,
    tick: Tick,
    bot: Option<MatchBot>,
}

/// A bot a play drives by the step a server's bot driver takes: its slot, its script, the lookup
/// of its units the driver's is and the step's buffers, the payloads a step gives, the point its
/// command center stands at, and the farthest any of its units stood from there.
#[derive(Debug)]
struct MatchBot {
    slot: u32,
    script: BotScript,
    players: HeldPlayerUnits,
    buffers: BotBuffers,
    due: DuePayloads,
    home: Vec3,
    farthest: Num,
}

/// The payloads a bot's step gives in a tick, which the play serves once the step, which reads
/// the match, is done; an order of no unit the server's driver drops goes unsent here too.
#[derive(Debug, Default)]
struct DuePayloads(Vec<Vec<u8>>);

impl BotSink for DuePayloads {
    fn send(&mut self, _: PlayerSlot, payload: &[u8]) {
        self.0.push(payload.to_vec());
    }

    fn unordered(&mut self, _: PlayerSlot, _: Action, _: UnitsMissing) {}
}

impl MatchPlay {
    /// `fixed`, before its first tick.
    pub(crate) const fn new(fixed: FixedMatch) -> Self {
        Self {
            fixed,
            tick: Tick::new(0),
            bot: None,
        }
    }

    /// Drives player `slot` by `script` from the tick it runs next, as a server drives its bot,
    /// and keeps the farthest its units stand from its command center; a failure when the player
    /// has no one center.
    pub(crate) fn drive_bot(&mut self, slot: u32, script: BotScript) -> Result<(), Failure> {
        let [center] = self.owned(CENTER, slot)?[..] else {
            return Err(Failure::FightStart);
        };
        let home = MatchUnits::of(self.game()).pos(center);
        self.bot = Some(MatchBot {
            slot,
            script,
            players: HeldPlayerUnits::new(self.fixed.game_mut()),
            buffers: BotBuffers::default(),
            due: DuePayloads::default(),
            home,
            farthest: Num::ZERO,
        });
        Ok(())
    }

    /// The farthest a unit of the bot stood from its command center, on the ground plane, in
    /// meters; none with no bot.
    pub(crate) fn bot_farthest(&self) -> Option<Num> {
        self.bot.as_ref().map(|bot| bot.farthest)
    }

    /// The tick it runs next: how many ticks it ran.
    pub(crate) const fn tick(&self) -> Tick {
        self.tick
    }

    pub(crate) const fn game(&self) -> &Match {
        self.fixed.game()
    }

    /// Logs the bot's orders its script has due, then runs the tick, and moves on to the next.
    pub(crate) fn step(&mut self) {
        if let Some(mut bot) = self.bot.take() {
            self.serve_bot(&mut bot);
            self.bot = Some(bot);
        }
        self.fixed.game_mut().run_tick_alone();
        self.tick = Tick::new(self.tick.get() + 1);
        if let Some(bot) = &mut self.bot {
            let units = MatchUnits::of(self.fixed.game());
            let slot = bot.slot;
            for (_, unit) in units
                .all()
                .filter(|(_, unit)| MatchUnits::owned_by(unit, slot))
            {
                let Some(pos) = unit.get::<Position>() else {
                    continue;
                };
                let distance = bot.home.ground_offset(pos.get()).length();
                bot.farthest = bot.farthest.max(distance);
            }
        }
    }

    /// Logs what `bot`'s script has due by the tick it runs next, as the server's bot driver's
    /// step gives it: its mode inputs, then its orders, each for the bot's units it names.
    fn serve_bot(&mut self, bot: &mut MatchBot) {
        let MatchBot {
            slot,
            script,
            players,
            buffers,
            due,
            ..
        } = bot;
        let slot = PlayerSlot::new(*slot);
        let units = players.get(self.fixed.game());
        script.step(self.tick, slot, &units, buffers, due);
        for payload in due.0.drain(..) {
            let input = ServerInput::Bot {
                slot,
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

    /// The living units player `slot` owns of the type `name`, by stable id; a failure when the
    /// mode has no such type.
    pub(crate) fn owned(&self, name: &'static str, slot: u32) -> Result<Vec<StableId>, Failure> {
        let world = self.fixed.game();
        let typed: UnitType = Units::type_named(world, name).ok_or(Failure::NoType(name))?;
        let owned = MatchUnits::of(world)
            .all()
            .filter(|(_, unit)| {
                unit.get::<UnitType>() == Some(&typed)
                    && MatchUnits::owned_by(unit, slot)
                    && unit.get::<Life>().is_none_or(|life| !life.is_empty())
            })
            .map(|(id, _)| id)
            .collect();
        Ok(owned)
    }

    /// Ends the match: it takes the log's events while it lives, as its harness checks them, so
    /// it ends before the check logs again.
    pub(crate) fn end(mut self) {
        self.fixed.end();
    }
}
