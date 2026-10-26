use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_capabilities::{BotCall, OnBotThink};
use campfire_common::{DeclaredName, PlayerSlot, Tick, Ticks};
use campfire_mode::{ModeParam, ModeWorld};
use campfire_package::ModeBot;
use campfire_script::{Budget, CallError, CallStart, Ctx, ScriptId, ScriptsWorld};
use campfire_sim::MatchRate;
use campfire_values::PlayerView;

use crate::bot_script::bot_names::BotNames;
use crate::bot_script::error::ThinkError;
use crate::bot_script::{BotBuffers, BotOrder, BotSink};
use crate::bot_view::HeldBotView;

/// A bot of its game's script, as a mode names it: the script it thinks by, its params, and the
/// time between two thinks (03-game-scripting.md, A bot is a Rhai script of its game, which the
/// server's bot driver runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingBot {
    script: ScriptId,
    /// Shared with the bot's call at each think, which converts a param as a script reads it.
    params: Arc<[(DeclaredName, ModeParam)]>,
    think_ms: NonZeroU32,
}

/// A thinking bot as a driver plays it: the bot, the period of its think, and whether it waits to
/// think in the next tick, as its last think found the tick's budget spent (03-game-scripting.md,
/// The thinks of a tick share a budget).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacedBot {
    bot: ThinkingBot,
    period: Ticks,
    waits: bool,
}

/// What one think of a bot reads and where its orders go: its player slot, the tick it thinks
/// before, the budget it draws from, the match's names its orders resolve in, the view its orders
/// read, the buffers of their payloads, and the sink that takes them.
#[derive(Debug)]
pub struct Think<'a, S: BotSink> {
    pub player_slot: PlayerSlot,
    pub now: Tick,
    pub budget: &'a mut Budget,
    pub names: &'a BotNames<'a>,
    pub view: &'a mut HeldBotView,
    pub buffers: &'a mut BotBuffers,
    pub sink: &'a mut S,
}

impl ThinkingBot {
    /// The bot `bot` names.
    pub fn new(bot: ModeBot<'_>) -> Self {
        Self {
            script: bot.script,
            params: bot
                .params
                .iter()
                .map(|(name, param)| (name.clone(), param.clone()))
                .collect(),
            think_ms: bot.think_ms,
        }
    }

    /// The period of its think in a match at `rate`: in whole ticks, rounded up, a tick at the
    /// least.
    fn period(&self, rate: MatchRate) -> Ticks {
        rate.duration(NonZeroU64::from(self.think_ms).get())
            .expect("a think's period of a u32 of milliseconds fits in ticks")
    }

    /// Thinks once in the match of `world` as `think` says: calls the bot's script with its
    /// params, then sends each order it gave, for its units as the view holds them, as an order
    /// script's step sends its orders; an error, and no order sent, when its call fails or an
    /// order names what the match lacks.
    pub(crate) fn think<S: BotSink>(
        &self,
        world: &mut World,
        think: Think<'_, S>,
    ) -> Result<(), ThinkError> {
        let Think {
            player_slot,
            now,
            budget,
            names,
            view,
            buffers,
            sink,
        } = think;
        let team = world.team_of(player_slot);
        let player = PlayerView {
            player_slot,
            team: team.expect("a bot plays a player slot of the match's mode"),
            out: false,
        };
        {
            let ctx = world.non_send::<Ctx>().clone();
            let mut frame = ctx.frame();
            let call = frame
                .extension_mut::<BotCall>()
                .expect("a match that runs scripts holds a bot's call");
            call.ready(&self.params);
            view.get(world)
                .facts(player_slot, now, &mut buffers.owned, call.facts_mut());
        }
        let given = &mut buffers.given;
        given.clear();
        world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            batch.run(CallStart::bot(player, now), |batch| {
                batch.call_from::<OnBotThink>(budget, self.script, (ctx.clone(),))
            })?;
            ctx.frame()
                .extension_mut::<BotCall>()
                .expect("the frame holds a bot's call")
                .take_orders(given);
            Ok::<_, CallError>(())
        })?;
        let orders = given
            .drain(..)
            .map(|order| BotOrder::of_think(order, now, names))
            .collect::<Result<Vec<_>, _>>()?;
        let view = view.get(world);
        for order in &orders {
            order.send(player_slot, &view, buffers, sink);
        }
        Ok(())
    }
}

impl PacedBot {
    /// `bot` in a match at `rate`.
    pub fn new(bot: ThinkingBot, rate: MatchRate) -> Self {
        Self {
            period: bot.period(rate),
            bot,
            waits: false,
        }
    }

    /// Thinks as `think` says when the think is due before its tick: the bot waits from a think
    /// that found the budget spent, and thinks in the next tick; the error of any other think that
    /// failed.
    pub fn play<S: BotSink>(
        &mut self,
        world: &mut World,
        think: Think<'_, S>,
    ) -> Result<(), ThinkError> {
        if !self.waits && !Self::in_phase(think.player_slot, think.now, self.period) {
            return Ok(());
        }
        let thought = self.bot.think(world, think);
        self.waits = matches!(thought, Err(ThinkError::Call(CallError::BudgetSpent)));
        if self.waits {
            return Ok(());
        }
        thought
    }

    /// Whether the bot of `player_slot` that thinks every `period` thinks before `tick`: player
    /// slot `s` first in tick `s`, each player slot a tick later in the period than the player slot
    /// before, so bots of one period think in ticks of their own, and only player slots a whole
    /// period apart share one (03-game-scripting.md, A bot decides from what it sees, each think).
    const fn in_phase(player_slot: PlayerSlot, tick: Tick, period: Ticks) -> bool {
        match tick.get().checked_sub(player_slot.get() as u64) {
            Some(since) => since.is_multiple_of(period.get()),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests;
