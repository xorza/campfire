use bevy_ecs::resource::Resource;
use campfire_capabilities::{Directive, NamedRefs, Order, OrderPayloads, ThinkOrder};
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_players::OrderUnits;
use campfire_production::PlaceSearch;
use campfire_script::Filter;
use campfire_sim::StableId;
use campfire_units::{GroundPoint, Ids, RefMap};
use campfire_values::{ItemId, UnitType};
use campfire_vision::TeamSight;

use crate::bot_script::bot_names::BotNames;
use crate::bot_script::error::{BotScriptError, OrderUnsent, ThinkError};
use crate::bot_view::{BotView, UnitRole};
use crate::loaded::{ActionName, Loaded, LoadedPlace};
use crate::order_script::{OrderScript, ScriptedInput, ScriptedUnits};

pub(crate) mod bot_names;
pub(crate) mod error;

/// A script a bot plays for its player, each name it holds resolved in the match's books and map:
/// each order goes out as it is due, for the player's units it names, and each mode input, in
/// the tick of its stamp, with the orders the player gives by hand.
#[derive(Resource, Debug, Clone)]
pub struct BotScript {
    orders: Vec<BotOrder>,
    inputs: Vec<ScriptedInput>,
    end: Option<Tick>,
    /// The first order not sent yet.
    next_order: usize,
    /// The first input not sent yet.
    next_input: usize,
}

/// One order of a bot's script: the sim tick it is sent in, the units it commands, what they do,
/// and that directive as the script names it, which a report of an unsent order gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotOrder {
    pub(crate) tick: Tick,
    pub units: BotUnits,
    directive: Directive<Loaded>,
    /// The slot it names, when it names one, which each commanded unit finds among its own.
    slot: Option<ActionName>,
    pub named: Directive<NamedRefs>,
}

/// Where a bot's step sends what its script has due: each payload, a mode input's or an order's,
/// as its player's client writes it, and each order that goes unsent.
pub trait BotSink {
    /// Takes `payload`, an input of the bot of `player_slot`.
    fn send(&mut self, player_slot: PlayerSlot, payload: &[u8]);

    /// The most bytes an input's payload may hold, as the session's terms give it.
    fn max_payload_len(&self) -> u32;

    /// Hears that the order of `directive`, as its script names it, of the bot of `player_slot`
    /// goes unsent to all of its units or to some, as `cause` says.
    fn unsent(
        &mut self,
        player_slot: PlayerSlot,
        directive: &Directive<NamedRefs>,
        cause: OrderUnsent,
    );

    /// Hears that a think of the bot of `player_slot` gave no orders, as `error` says.
    fn failed(&mut self, player_slot: PlayerSlot, error: ThinkError);
}

/// The buffers of a bot's step, kept so that no input allocates: a mode input's payload and the
/// body of the command in it, an order's payloads, the orders of a due one, and what a think
/// reads of its units.
#[derive(Debug, Default)]
pub struct BotBuffers {
    payload: Vec<u8>,
    body: Vec<u8>,
    order_payloads: OrderPayloads,
    orders: Vec<Order>,
    scratch: OrderScratch,
    /// The units the thinking bot's player slot owns, which its facts read.
    pub(crate) owned: Vec<StableId>,
    /// The orders a think gave.
    pub(crate) given: Vec<ThinkOrder>,
}

/// The buffers of the orders a due one gives: its units, and each with the place of its slot.
#[derive(Debug, Default)]
pub struct OrderScratch {
    units: Vec<StableId>,
    slotted: Vec<(u8, StableId)>,
}

/// The units of its player a bot's order commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotUnits {
    /// The player's avatar.
    Avatar,
    /// The player's living units of the type, as the order is due.
    Typed(UnitType),
    /// Those of the units a bot's think names, by stable id, rising, that the player owns and
    /// live.
    Listed(Box<[StableId]>),
}

/// Gives each reference of a due order its id: the slot the order's units hold, and the nearest
/// unit its filter selects from the first of them, which one of them may attack for an attack.
#[derive(Debug)]
struct DueRefs<'v, 'w, 's, S: TeamSight> {
    view: &'v BotView<'w, 's, S>,
    units: &'v [StableId],
    slot: Option<u8>,
}

impl BotScript {
    /// The bot that plays `script`, each name it holds one of `names`; an error for the first it
    /// holds that the match does not have.
    pub fn new(script: &OrderScript, names: &BotNames<'_>) -> Result<Self, BotScriptError> {
        let orders = script.orders().iter().map(|order| {
            let units = match &order.units {
                ScriptedUnits::Avatar => BotUnits::Avatar,
                ScriptedUnits::Typed(name) => BotUnits::Typed(names.unit_type(name)?),
            };
            let mut refs = names.refs();
            let directive = order.directive.clone().map_refs(&mut refs)?;
            Ok(BotOrder {
                tick: order.tick,
                units,
                directive,
                slot: refs.slot,
                named: order.directive.clone(),
            })
        });
        for input in script.inputs() {
            names.input(input)?;
        }
        Ok(Self {
            orders: orders.collect::<Result<_, BotScriptError>>()?,
            inputs: script.inputs().to_vec(),
            end: script.end(),
            next_order: 0,
            next_input: 0,
        })
    }

    /// The last tick the player plays, when the script names one.
    pub const fn end(&self) -> Option<Tick> {
        self.end
    }

    /// Takes the mode inputs due by `tick`, each once: a tick that passed while the match did not
    /// run sends its own late.
    pub fn due_inputs(&mut self, tick: Tick) -> &[ScriptedInput] {
        let first = self.next_input;
        self.next_input += self.inputs[first..].partition_point(|input| input.tick <= tick);
        &self.inputs[first..self.next_input]
    }

    /// Sends `sink` what the script has due by `tick` for the bot of `player_slot`: its mode
    /// inputs, then its orders, each for the slot's units as `view` holds them now; an order that
    /// goes unsent `sink` hears of. The one step of a bot, which the server's driver and every
    /// check take.
    pub fn step<S: TeamSight>(
        &mut self,
        tick: Tick,
        player_slot: PlayerSlot,
        view: &BotView<'_, '_, S>,
        buffers: &mut BotBuffers,
        sink: &mut impl BotSink,
    ) {
        let BotBuffers { payload, body, .. } = buffers;
        for input in self.due_inputs(tick) {
            payload.clear();
            input.write_payload(body, payload);
            sink.send(player_slot, payload);
        }
        for scripted in self.due_orders(tick) {
            scripted.send(player_slot, view, buffers, sink);
        }
    }

    /// Takes the orders due by `tick`, each once, as `due_inputs` takes the mode inputs.
    pub fn due_orders(&mut self, tick: Tick) -> &[BotOrder] {
        self.due_orders_while(tick, |_| true)
    }

    /// Takes the orders due by `tick` as `due_orders` does, but only up to the first that is not
    /// `ready`, which waits with those after it.
    pub fn due_orders_while(
        &mut self,
        tick: Tick,
        mut ready: impl FnMut(&BotOrder) -> bool,
    ) -> &[BotOrder] {
        let first = self.next_order;
        let due = self.orders[first..].partition_point(|order| order.tick <= tick);
        let taken = self.orders[first..first + due]
            .iter()
            .take_while(|order| ready(order))
            .count();
        self.next_order += taken;
        &self.orders[first..self.next_order]
    }
}

impl BotOrder {
    /// The order a bot's think gave in tick `tick`, each name it holds one of `names`; an error
    /// for the first it holds that the match does not have.
    pub fn of_think(
        order: ThinkOrder,
        tick: Tick,
        names: &BotNames<'_>,
    ) -> Result<Self, BotScriptError> {
        let ThinkOrder {
            mut units,
            directive,
        } = order;
        units.sort_unstable();
        units.dedup();
        let mut refs = names.refs();
        let loaded = directive.clone().map_refs(&mut refs)?;
        Ok(Self {
            tick,
            units: BotUnits::Listed(units.into()),
            directive: loaded,
            slot: refs.slot,
            named: directive,
        })
    }

    /// Sends `sink` the payload of each order it gives for the player of `player_slot`, for its
    /// units as `view` holds them now, and why it goes unsent to all of them or to some.
    pub fn send<S: TeamSight>(
        &self,
        player_slot: PlayerSlot,
        view: &BotView<'_, '_, S>,
        buffers: &mut BotBuffers,
        sink: &mut impl BotSink,
    ) {
        let BotBuffers {
            order_payloads,
            orders,
            scratch,
            ..
        } = buffers;
        orders.clear();
        let unsent = self.orders(player_slot, view, scratch, orders);
        let max_len = sink.max_payload_len();
        for order in orders.iter() {
            let fits = order.write_payloads(max_len, order_payloads);
            assert!(
                fits,
                "an order fits payloads, as the session's terms hold every input"
            );
            for payload in &*order_payloads {
                sink.send(player_slot, payload);
            }
        }
        if let Some(cause) = unsent {
            sink.unsent(player_slot, &self.named, cause);
        }
    }

    /// Puts in `out` the orders it gives for the player of `player_slot`, for its units as `view`
    /// holds them now: one, or for an action of a slot one for each place among their slots its
    /// units hold the action at, each to the units that hold it there, in increasing stable id; and
    /// why it goes unsent to all of its units or to some, the first cause of several.
    pub fn orders<S: TeamSight>(
        &self,
        player_slot: PlayerSlot,
        view: &BotView<'_, '_, S>,
        scratch: &mut OrderScratch,
        out: &mut Vec<Order>,
    ) -> Option<OrderUnsent> {
        let OrderScratch { units, slotted } = scratch;
        units.clear();
        match &self.units {
            BotUnits::Avatar => match view.avatar(player_slot) {
                Some(avatar) => units.push(avatar.id),
                None => return Some(OrderUnsent::NoAvatar),
            },
            &BotUnits::Typed(unit_type) => view.typed(player_slot, unit_type, units),
            BotUnits::Listed(ids) => view.listed(player_slot, ids, units),
        }
        if units.is_empty() {
            return Some(OrderUnsent::NoUnit);
        }
        let Some(name) = &self.slot else {
            return self.give(units, None, view, out).err();
        };
        let missing = by_slot(units, |id| view.slot_of(id, name), slotted);
        let mut unsent = missing.then_some(OrderUnsent::NoSlot);
        for run in slotted.chunk_by(|a, b| a.0 == b.0) {
            units.clear();
            units.extend(run.iter().map(|&(_, id)| id));
            if let Err(cause) = self.give(units, Some(run[0].0), view, out) {
                unsent.get_or_insert(cause);
            }
        }
        unsent
    }

    /// Puts in `out` its order to `units`, its slot at `slot` among theirs when it names one;
    /// why it has none.
    fn give<S: TeamSight>(
        &self,
        units: &[StableId],
        slot: Option<u8>,
        view: &BotView<'_, '_, S>,
        out: &mut Vec<Order>,
    ) -> Result<(), OrderUnsent> {
        let mut refs = DueRefs { view, units, slot };
        let directive = self.directive.clone().map_refs(&mut refs)?;
        let units = OrderUnits::new(units).expect("an order commands a unit at least");
        out.push(Order { units, directive });
        Ok(())
    }
}

/// Puts in `slotted` each of `units` that `slot_of` gives a place of the action among its slots,
/// with that place, in the order of the places, then of the stable ids; whether a unit has none.
fn by_slot(
    units: &[StableId],
    slot_of: impl Fn(StableId) -> Option<u8>,
    slotted: &mut Vec<(u8, StableId)>,
) -> bool {
    slotted.clear();
    slotted.extend(units.iter().filter_map(|&id| Some((slot_of(id)?, id))));
    slotted.sort_unstable();
    slotted.len() < units.len()
}

impl<S: TeamSight> DueRefs<'_, '_, '_, S> {
    /// The place among its units' slots of the action the order names.
    const fn order_slot(&self) -> u8 {
        self.slot
            .expect("an order of a slot finds its units' place of it first")
    }

    /// The nearest unit `filter` selects that the order takes as `role`.
    fn nearest(&self, role: UnitRole, filter: Filter) -> Result<StableId, OrderUnsent> {
        self.view
            .nearest(self.units, role, filter)
            .ok_or(OrderUnsent::NoTarget)
    }
}

impl<S: TeamSight> RefMap<Loaded, Ids> for DueRefs<'_, '_, '_, S> {
    type Error = OrderUnsent;

    fn unit(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Seen, unit)
    }

    fn attack(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Attacked, unit)
    }

    fn aimed(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Aimed(self.order_slot()), unit)
    }

    fn used(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Used, unit)
    }

    fn shop(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Shop, unit)
    }

    fn site(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.nearest(UnitRole::Site(self.order_slot()), unit)
    }

    fn slot(&mut self, _: ActionName) -> Result<u8, OrderUnsent> {
        Ok(self.order_slot())
    }

    fn ground(&mut self, ground: GroundPoint) -> Result<GroundPoint, OrderUnsent> {
        Ok(ground)
    }

    /// The point, or the nearest of its search where the build of the order's slot passes, for
    /// its first unit, the builder, as the order goes out.
    fn place(&mut self, place: LoadedPlace, angle: Num) -> Result<GroundPoint, OrderUnsent> {
        let (near, gap, steps) = match place {
            LoadedPlace::At(at) => return Ok(at),
            LoadedPlace::Near { near, gap, steps } => (near, gap, steps),
        };
        let search = PlaceSearch {
            builder: self.units[0],
            slot: self.order_slot(),
            near,
            gap,
            steps,
            angle,
        };
        self.view.place(search).ok_or(OrderUnsent::NoPlace)
    }

    fn point(&mut self, point: Vec3) -> Result<Vec3, OrderUnsent> {
        Ok(point)
    }

    fn item(&mut self, item: ItemId) -> Result<ItemId, OrderUnsent> {
        Ok(item)
    }

    /// The first slot of the order's first unit that carries a stack of `item`, as the order goes
    /// out.
    fn carried(&mut self, item: ItemId) -> Result<u8, OrderUnsent> {
        self.view
            .carried(self.units[0], item)
            .ok_or(OrderUnsent::NoStack)
    }
}

#[cfg(test)]
pub(crate) mod tests;
