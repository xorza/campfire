use bevy_ecs::world::World;
use campfire_script::{
    CallError, CallExtension, CallParams, CallStart, Commit, ParamScope, ScriptRole, ScriptValue,
};
use campfire_sim::StableId;
use campfire_values::{ActionId, Scalar};

use crate::order::Directive;
use crate::order::named_refs::NamedRefs;

/// What a bot's think reads besides the match, and what it gives: the bot's own params, which lie
/// over the mode's, the trains and builds its units may start, and the orders it gives, which
/// its driver takes once the think returns (04-capabilities/control.md, A bot orders by the order
/// scripts' directives).
#[derive(Debug, Default)]
pub struct BotCall {
    /// Each param of the thinking bot, by name, as `ctx.params` reads it.
    params: Vec<(String, ScriptValue)>,
    /// Each unit of the bot, by stable id, with each action of its slots it may start, the units
    /// rising.
    startable: Vec<(StableId, ActionId)>,
    /// The bot's units that stand idle, rising.
    idle: Vec<StableId>,
    orders: Vec<ThinkOrder>,
}

/// What a bot's think reads of its units that only the world's systems tell, as its driver finds
/// it before the think: each unit, by stable id, with each action of its slots it may start, the
/// units rising, and the units that stand idle, rising (03-game-scripting.md, A bot reads what its
/// units may start; 04-capabilities/control.md, A bot gathers).
#[derive(Debug, Clone, Copy)]
pub struct BotFacts<'a> {
    pub startable: &'a [(StableId, ActionId)],
    pub idle: &'a [StableId],
}

/// An order a bot's think gives: the units it names, by stable id, and the directive, in the
/// order scripts' names, which the driver resolves as it resolves an order script's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkOrder {
    pub units: Vec<StableId>,
    pub directive: Directive<NamedRefs>,
}

impl BotCall {
    /// Readies it for the think of a bot of `params`, each by its name, whose units are as
    /// `facts` finds them.
    pub fn ready(
        &mut self,
        params: impl IntoIterator<Item = (String, ScriptValue)>,
        facts: BotFacts<'_>,
    ) {
        let BotFacts { startable, idle } = facts;
        debug_assert!(
            startable.is_sorted_by_key(|&(id, _)| id) && idle.is_sorted(),
            "the units of a bot's facts rise"
        );
        self.params.clear();
        self.params.extend(params);
        self.startable.clear();
        self.startable.extend_from_slice(startable);
        self.idle.clear();
        self.idle.extend_from_slice(idle);
    }

    /// Whether the unit `id` stood idle as the think began.
    pub(crate) fn idle(&self, id: StableId) -> bool {
        self.idle.binary_search(&id).is_ok()
    }

    /// The actions the unit `id` may start, as its think began.
    pub(crate) fn startable(&self, id: StableId) -> impl Iterator<Item = ActionId> + '_ {
        let from = self.startable.partition_point(|&(unit, _)| unit < id);
        let held = self.startable[from..].iter();
        held.take_while(move |&&(unit, _)| unit == id)
            .map(|&(_, action)| action)
    }

    /// Moves the orders the last think gave into `out`, in the order it gave them.
    pub fn take_orders(&mut self, out: &mut Vec<ThinkOrder>) {
        out.append(&mut self.orders);
    }

    /// Queues `order`, which the driver takes once the think returns.
    pub(crate) fn give(&mut self, order: ThinkOrder) {
        self.orders.push(order);
    }
}

impl CallExtension for BotCall {
    /// After: a bot's think applies nothing, so nothing commits.
    const COMMITS: Commit = Commit::AfterEffects;

    fn begin_batch(&mut self, _: &World) {}

    /// A bot's think starts with no order given.
    fn begin(&mut self, _: &World, start: &CallStart) -> Result<(), CallError> {
        if start.role == ScriptRole::Bot {
            self.orders.clear();
        }
        Ok(())
    }

    fn commit(&mut self, _: &mut World) {}
}

impl CallParams for BotCall {
    const SCOPE: ParamScope = ParamScope::Bot;

    fn param(&self, name: &str) -> Option<ScriptValue> {
        let held = self.params.iter().find(|(held, _)| held == name);
        held.map(|(_, value)| value.clone())
    }

    /// None: no effect list numbers a bot's param by its place.
    fn param_at(&self, _: usize) -> Option<Scalar> {
        None
    }
}
