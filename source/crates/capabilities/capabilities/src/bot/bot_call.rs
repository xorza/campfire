use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_common::DeclaredName;
use campfire_mode::ModeParam;
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
    /// Each param of the thinking bot, by name, which `ctx.params` reads as a script value.
    params: Arc<[(DeclaredName, ModeParam)]>,
    /// `params` as script values, converted once for each set of params a think readies, as the
    /// mode's params are, so a read clones a value and converts none.
    values: Vec<(DeclaredName, ScriptValue)>,
    /// Each unit of the bot, by stable id, with each action of its slots it may start, the units
    /// rising.
    startable: Vec<(StableId, ActionId)>,
    /// The bot's units that stand idle, rising.
    idle: Vec<StableId>,
    orders: Vec<ThinkOrder>,
}

/// What a bot's think reads of its units that only the world's systems tell, which its driver
/// writes before the think: each unit, by stable id, with each action of its slots it may start,
/// the units rising, and the units that stand idle, rising (03-game-scripting.md, A bot reads what
/// its units may start; 04-capabilities/control.md, A bot gathers).
#[derive(Debug)]
pub struct BotFacts<'a> {
    pub startable: &'a mut Vec<(StableId, ActionId)>,
    pub idle: &'a mut Vec<StableId>,
}

/// An order a bot's think gives: the units it names, by stable id, and the directive, in the
/// order scripts' names, which the driver resolves as it resolves an order script's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkOrder {
    pub units: Vec<StableId>,
    pub directive: Directive<NamedRefs>,
}

impl BotCall {
    /// Readies it for the think of a bot of `params`, which are shared: converted into script
    /// values once, when they are not the params of the last think.
    pub fn ready(&mut self, params: &Arc<[(DeclaredName, ModeParam)]>) {
        if Arc::ptr_eq(&self.params, params) && self.values.len() == params.len() {
            return;
        }
        self.params = Arc::clone(params);
        self.values.clear();
        let values = params
            .iter()
            .map(|(name, value)| (name.clone(), value.to_value()));
        self.values.extend(values);
    }

    /// Its facts, for the driver to write before the think, its units rising.
    pub const fn facts_mut(&mut self) -> BotFacts<'_> {
        BotFacts {
            startable: &mut self.startable,
            idle: &mut self.idle,
        }
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
        let held = self.values.iter().find(|(held, _)| held.as_str() == name);
        held.map(|(_, value)| value.clone())
    }

    /// None: no effect list numbers a bot's param by its place.
    fn param_at(&self, _: usize) -> Option<Scalar> {
        None
    }
}
