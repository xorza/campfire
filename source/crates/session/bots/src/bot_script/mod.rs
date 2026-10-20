use bevy_ecs::resource::Resource;
use campfire_capabilities::{Directive, Order};
use campfire_common::{Seat, Tick};
use campfire_math::Vec3;
use campfire_players::OrderUnits;
use campfire_script::Filter;
use campfire_sim::StableId;
use campfire_units::{GroundPoint, Ids, RefMap};
use campfire_values::{ItemId, UnitType};
use campfire_vision::TeamSight;

use crate::bot_script::bot_names::BotNames;
use crate::bot_script::error::{BotScriptError, OrderUnsent};
use crate::bot_view::BotView;
use crate::loaded::{ActionName, Loaded};
use crate::named::NamedRefs;
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
    /// Takes `payload`, an input of the bot of `seat`.
    fn send(&mut self, seat: Seat, payload: &[u8]);

    /// Hears that the order of `directive`, as its script names it, of the bot of `seat` goes
    /// unsent to all of its units or to some, as `cause` says.
    fn unsent(&mut self, seat: Seat, directive: &Directive<NamedRefs>, cause: OrderUnsent);
}

/// The buffers of a bot's step, kept so that no input allocates: the payload, the body of the
/// command in it, and the orders of a due one.
#[derive(Debug, Default)]
pub struct BotBuffers {
    payload: Vec<u8>,
    body: Vec<u8>,
    orders: Vec<Order>,
    scratch: OrderScratch,
}

/// The buffers of the orders a due one gives: its units, and each with the place of its slot.
#[derive(Debug, Default)]
pub struct OrderScratch {
    units: Vec<StableId>,
    slotted: Vec<(u8, StableId)>,
}

/// The units of its player a bot's order commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotUnits {
    /// The player's avatar.
    Avatar,
    /// The player's living units of the type, as the order is due.
    Typed(UnitType),
}

/// Gives each reference of a due order its id: the slot the order's units hold, and the nearest
/// unit its filter selects from the first of them.
#[derive(Debug)]
struct DueRefs<'v, 'w, 's, S: TeamSight> {
    view: &'v BotView<'w, 's, S>,
    from: StableId,
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

    /// Sends `sink` what the script has due by `tick` for the bot of `seat`: its mode inputs,
    /// then its orders, each for the slot's units as `view` holds them now; an order that goes
    /// unsent `sink` hears of. The one step of a bot, which the server's driver and every check
    /// take.
    pub fn step<S: TeamSight>(
        &mut self,
        tick: Tick,
        seat: Seat,
        view: &BotView<'_, '_, S>,
        buffers: &mut BotBuffers,
        sink: &mut impl BotSink,
    ) {
        let BotBuffers {
            payload,
            body,
            orders,
            scratch,
        } = buffers;
        for input in self.due_inputs(tick) {
            payload.clear();
            input.write_payload(body, payload);
            sink.send(seat, payload);
        }
        for scripted in self.due_orders(tick) {
            orders.clear();
            let unsent = scripted.orders(seat, view, scratch, orders);
            for order in orders.iter() {
                payload.clear();
                order.write_payload(body, payload);
                sink.send(seat, payload);
            }
            if let Some(cause) = unsent {
                sink.unsent(seat, &scripted.named, cause);
            }
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
    /// Puts in `out` the orders it gives for the player of `seat`, for its units as `view` holds
    /// them now: one, or for an action of a slot one for each place among their slots its units
    /// hold the action at, each to the units that hold it there, in increasing stable id; and
    /// why it goes unsent to all of its units or to some, the first cause of several.
    pub fn orders<S: TeamSight>(
        &self,
        seat: Seat,
        view: &BotView<'_, '_, S>,
        scratch: &mut OrderScratch,
        out: &mut Vec<Order>,
    ) -> Option<OrderUnsent> {
        let OrderScratch { units, slotted } = scratch;
        units.clear();
        match self.units {
            BotUnits::Avatar => match view.avatar(seat) {
                Some(avatar) => units.push(avatar.id),
                None => return Some(OrderUnsent::NoAvatar),
            },
            BotUnits::Typed(unit_type) => view.typed(seat, unit_type, units),
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
        let mut refs = DueRefs {
            view,
            from: units[0],
            slot,
        };
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

impl<S: TeamSight> RefMap<Loaded, Ids> for DueRefs<'_, '_, '_, S> {
    type Error = OrderUnsent;

    fn unit(&mut self, unit: Filter) -> Result<StableId, OrderUnsent> {
        self.view
            .nearest(self.from, unit)
            .ok_or(OrderUnsent::NoTarget)
    }

    fn slot(&mut self, _: ActionName) -> Result<u8, OrderUnsent> {
        Ok(self
            .slot
            .expect("an order of a slot finds its units' place of it first"))
    }

    fn ground(&mut self, ground: GroundPoint) -> Result<GroundPoint, OrderUnsent> {
        Ok(ground)
    }

    fn point(&mut self, point: Vec3) -> Result<Vec3, OrderUnsent> {
        Ok(point)
    }

    fn item(&mut self, item: ItemId) -> Result<ItemId, OrderUnsent> {
        Ok(item)
    }
}

#[cfg(test)]
mod tests;
