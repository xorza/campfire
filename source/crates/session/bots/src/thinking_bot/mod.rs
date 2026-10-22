use std::num::{NonZeroU32, NonZeroU64};

use bevy_ecs::world::World;
use campfire_capabilities::{BotCall, OnBotThink};
use campfire_common::{DeclaredName, Seat, Tick, Ticks};
use campfire_mode::ModeParam;
use campfire_package::ModeBot;
use campfire_script::{Budget, CallError, CallStart, ScriptId, ScriptsWorld};
use campfire_sim::MatchRate;

use crate::bot_script::bot_names::BotNames;
use crate::bot_script::error::BotScriptError;
use crate::bot_script::{BotBuffers, BotOrder, BotSink};
use crate::bot_view::HeldBotView;

/// A bot of its game's script, as a mode names it: the script it thinks by, its params, and the
/// time between two thinks (03-game-scripting.md, A bot is a Rhai script of its game, which the
/// server's bot driver runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingBot {
    script: ScriptId,
    params: Vec<(DeclaredName, ModeParam)>,
    think_ms: NonZeroU32,
}

/// A thinking bot as a driver plays it: the bot, the period of its think, and whether it waits
/// to think in the next tick, as its last think found the tick's pool spent (03-game-scripting.md,
/// The thinks of a tick share a pool).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacedBot {
    bot: ThinkingBot,
    period: Ticks,
    waits: bool,
}

/// What one think of a bot reads and where its orders go: its seat, the tick it thinks before,
/// the budget it draws from, the match's names its orders resolve in, the view its orders read,
/// the buffers of their payloads, and the sink that takes them.
#[derive(Debug)]
pub struct Think<'a, S: BotSink> {
    pub seat: Seat,
    pub now: Tick,
    pub budget: &'a mut Budget,
    pub names: &'a BotNames<'a>,
    pub view: &'a mut HeldBotView,
    pub buffers: &'a mut BotBuffers,
    pub sink: &'a mut S,
}

/// Why a think gave no orders: its call failed, or an order it gave names what the match lacks.
#[derive(Debug, thiserror::Error)]
pub enum ThinkError {
    #[error(transparent)]
    Call(#[from] CallError),
    #[error(transparent)]
    Names(#[from] BotScriptError),
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
            seat,
            now,
            budget,
            names,
            view,
            buffers,
            sink,
        } = think;
        view.get(world).facts(seat, now, &mut buffers.facts);
        let facts = buffers.facts.of_call();
        let given = world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            {
                let mut frame = ctx.frame();
                let params = self.params.iter();
                let params =
                    params.map(|(name, param)| (name.as_str().to_owned(), param.to_value()));
                frame
                    .extension_mut::<BotCall>()
                    .expect("a match that runs scripts holds a bot's call")
                    .ready(params, facts);
            }
            batch.run(CallStart::bot(seat, now), |batch| {
                batch.call_from::<OnBotThink>(budget, self.script, (ctx.clone(),))
            })?;
            let mut given = Vec::new();
            ctx.frame()
                .extension_mut::<BotCall>()
                .expect("the frame holds a bot's call")
                .take_orders(&mut given);
            Ok::<_, CallError>(given)
        })?;
        let orders = given
            .into_iter()
            .map(|order| BotOrder::of_think(order, now, names))
            .collect::<Result<Vec<_>, _>>()?;
        let view = view.get(world);
        for order in &orders {
            order.send(seat, &view, buffers, sink);
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
    /// that found the pool spent, and thinks in the next tick; the error of any other think that
    /// failed.
    pub fn play<S: BotSink>(
        &mut self,
        world: &mut World,
        think: Think<'_, S>,
    ) -> Result<(), ThinkError> {
        if !self.waits && !Self::in_phase(think.seat, think.now, self.period) {
            return Ok(());
        }
        let thought = self.bot.think(world, think);
        self.waits = matches!(thought, Err(ThinkError::Call(CallError::PoolSpent)));
        if self.waits {
            return Ok(());
        }
        thought
    }

    /// Whether the bot of `seat` that thinks every `period` thinks before `tick`: each seat a
    /// tick later in the period than the seat before, so bots of one period think in ticks of
    /// their own (03-game-scripting.md, A bot decides from what it sees, each think).
    const fn in_phase(seat: Seat, tick: Tick, period: Ticks) -> bool {
        (tick.get() + seat.get() as u64).is_multiple_of(period.get())
    }
}

#[cfg(test)]
mod tests;
