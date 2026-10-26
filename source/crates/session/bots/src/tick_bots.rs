use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_script::Budget;

use crate::bot_script::bot_names::BotNames;
use crate::bot_script::{BotBuffers, BotScript, BotSink};
use crate::bot_view::HeldBotView;
use crate::thinking_bot::{PacedBot, Think};

/// The bots of a match's player slots, played before each tick as the server plays them, and as
/// every check plays them by the same rule: each bot in turn, its orders and mode inputs to a sink;
/// the thinks of a tick drawing on one budget, the mode's `think` limit (03-game-scripting.md, The
/// thinks of a tick share a budget), the player slot that plays first turned each tick, so no
/// player slot keeps finding the budget spent by those before it. It holds the view the bots read,
/// and their buffers, kept so a tick of no think allocates nothing.
#[derive(Debug)]
pub struct TickBots {
    bots: Vec<TickBot>,
    view: HeldBotView,
    buffers: BotBuffers,
    think_limit: u64,
}

/// What the thinks of a tick spent: the most operations one of them ran.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickThinks {
    pub most: u64,
}

/// A bot of a player slot, and what plays it.
#[derive(Debug)]
pub struct TickBot {
    pub player_slot: PlayerSlot,
    pub play: PlayerSlotPlay,
}

/// What plays a bot's player slot: an order script, its ticks counted from `since`; or a bot of the
/// mode, at its pace (08-sessions.md, A mode names its bots).
#[derive(Debug)]
pub enum PlayerSlotPlay {
    Orders { script: BotScript, since: Tick },
    Thinks(PacedBot),
}

impl TickBots {
    /// No bot yet, of the match of `world`, whose thinks share a budget of `think_limit`
    /// operations a tick.
    pub fn new(world: &mut World, think_limit: u64) -> Self {
        Self {
            bots: Vec::new(),
            view: HeldBotView::new(world),
            buffers: BotBuffers::default(),
            think_limit,
        }
    }

    /// Plays `bot` from the next tick, after the bots before it.
    pub fn push(&mut self, bot: TickBot) {
        self.bots.push(bot);
    }

    /// Keeps the bots `keep` holds to.
    pub fn retain(&mut self, keep: impl FnMut(&TickBot) -> bool) {
        self.bots.retain(keep);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, TickBot> {
        self.bots.iter()
    }

    /// Plays each bot before `tick` in the match of `world`, its orders' names resolved in
    /// `names`: an order script's mode inputs then its orders due by `tick`, a bot's think when it
    /// is due; each payload and each failed think to `sink`. The bot at `tick` modulo their count
    /// plays first, the rest in their order after it. What the tick's thinks spent.
    pub fn play<S: BotSink>(
        &mut self,
        world: &mut World,
        tick: Tick,
        names: &BotNames<'_>,
        sink: &mut S,
    ) -> TickThinks {
        let Self {
            bots,
            view,
            buffers,
            think_limit,
        } = self;
        let mut thinks = TickThinks::default();
        let Some(count) = u64::try_from(bots.len()).ok().filter(|&count| count > 0) else {
            return thinks;
        };
        let first = usize::try_from(tick.get() % count).expect("a place among the bots");
        let (before_first, from_first) = bots.split_at_mut(first);
        let mut budget = Budget::new(*think_limit);
        for bot in from_first.iter_mut().chain(before_first) {
            let player_slot = bot.player_slot;
            match &mut bot.play {
                PlayerSlotPlay::Orders { script, since } => {
                    let since = tick.since(*since).expect("a bot plays from its start on");
                    let view = view.get(world);
                    script.step(Tick::new(since.get()), player_slot, &view, buffers, sink);
                }
                PlayerSlotPlay::Thinks(paced) => {
                    let left = budget.left();
                    let think = Think {
                        player_slot,
                        now: tick,
                        budget: &mut budget,
                        names,
                        view,
                        buffers,
                        sink,
                    };
                    let played = paced.play(world, think);
                    thinks.most = thinks.most.max(left - budget.left());
                    if let Err(error) = played {
                        sink.failed(player_slot, error);
                    }
                }
            }
        }
        thinks
    }
}

impl PlayerSlotPlay {
    /// The order script `script` from tick `since`, played from tick `next` on: what the script
    /// had for earlier ticks was played already, by the run a restore follows.
    pub fn orders(mut script: BotScript, since: Tick, next: Tick) -> Self {
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due_inputs(Tick::new(before));
            script.due_orders(Tick::new(before));
        }
        Self::Orders { script, since }
    }
}

impl<'a> IntoIterator for &'a TickBots {
    type Item = &'a TickBot;
    type IntoIter = std::slice::Iter<'a, TickBot>;

    fn into_iter(self) -> Self::IntoIter {
        self.bots.iter()
    }
}
