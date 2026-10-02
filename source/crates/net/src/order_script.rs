use campfire_capabilities::{Action, ActionTarget, Scalar};
use campfire_math::Tick;
use campfire_sim::StableId;
use serde::Deserialize;

use crate::error::OrderScriptError;

/// A player's orders for their avatar, each at a sim tick, in tick order, and optionally the tick
/// the player leaves after: what a bot plays. It reads from TOML: an optional `end`, and one
/// `[[order]]` table each, with its `tick` and one action: `move = [x, z]` in meters,
/// `attack = <unit>`, or `cast = <slot>`, with `target = <unit>` when the ability takes one. A
/// unit is its stable id, which a mode that spawns in a fixed order gives each match alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderScript {
    orders: Vec<ScriptedOrder>,
    end: Option<Tick>,
}

/// One order of a script: the sim tick it is sent in, and what the avatar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedOrder {
    pub tick: Tick,
    pub action: Action,
}

impl OrderScript {
    /// The script `text` holds; an error when it is not TOML of this shape, when an order names
    /// no action or two, or a target without a cast, when a coordinate is past what a sim number
    /// holds, when the ticks do not grow, or when the script ends before an order.
    pub fn parse(text: &str) -> Result<OrderScript, OrderScriptError> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            end: Option<u64>,
            #[serde(default)]
            order: Vec<Entry>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            tick: u64,
            #[serde(rename = "move")]
            to: Option<[Scalar; 2]>,
            attack: Option<StableId>,
            cast: Option<u8>,
            target: Option<StableId>,
        }
        let file: File = toml::from_str(text).map_err(OrderScriptError::Toml)?;
        let mut orders = Vec::with_capacity(file.order.len());
        for Entry {
            tick,
            to,
            attack,
            cast,
            target,
        } in file.order
        {
            let action = match (to, attack, cast, target) {
                (Some(to), None, None, None) => {
                    let [x, z] = to.map(Scalar::to_num);
                    let (Some(x), Some(z)) = (x, z) else {
                        return Err(OrderScriptError::Coordinate { tick });
                    };
                    Action::Move { x, z }
                }
                (None, Some(target), None, None) => Action::Attack { target },
                (None, None, Some(slot), target) => Action::Slot {
                    slot,
                    target: target.map_or(ActionTarget::None, ActionTarget::Unit),
                },
                _ => return Err(OrderScriptError::Action { tick }),
            };
            let tick = Tick::new(tick);
            if orders
                .last()
                .is_some_and(|last: &ScriptedOrder| last.tick > tick)
            {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            orders.push(ScriptedOrder { tick, action });
        }
        let end = file.end.map(Tick::new);
        if let (Some(end), Some(last)) = (end, orders.last())
            && end < last.tick
        {
            return Err(OrderScriptError::EndsEarly { end: end.get() });
        }
        Ok(OrderScript { orders, end })
    }

    pub fn orders(&self) -> &[ScriptedOrder] {
        &self.orders
    }

    /// The last tick the player plays, when the script names one.
    pub const fn end(&self) -> Option<Tick> {
        self.end
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_script_reads_its_orders_in_tick_order_or_its_flaw() {
        let script = OrderScript::parse(
            "[[order]]\ntick = 30\nmove = [4, 0]\n\n[[order]]\ntick = 30\nmove = [\"-2.5\", 1]\n",
        )
        .unwrap();
        let half = Num::from_bits(1 << (Num::FRAC_BITS - 1));
        let num = |value| Num::from_int(value).unwrap();
        assert_eq!(
            script.orders(),
            [
                ScriptedOrder {
                    tick: Tick::new(30),
                    action: Action::Move {
                        x: num(4),
                        z: Num::ZERO
                    },
                },
                ScriptedOrder {
                    tick: Tick::new(30),
                    action: Action::Move {
                        x: num(-3) + half,
                        z: num(1)
                    },
                },
            ]
        );
        assert_eq!(script.end(), None);
        let empty = OrderScript::parse("").unwrap();
        assert_eq!((empty.orders(), empty.end()), (&[][..], None));
        // The script may end in the tick of its last order, not before.
        let ending = |end: u64| {
            OrderScript::parse(&format!(
                "end = {end}\n[[order]]\ntick = 9\nmove = [1, 1]\n"
            ))
        };
        assert_eq!(ending(9).unwrap().end(), Some(Tick::new(9)));
        assert_eq!(ending(8), Err(OrderScriptError::EndsEarly { end: 8 }));
        let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
        assert_eq!(
            flaw("[[order]]\ntick = 9\nmove = [1, 1]\n[[order]]\ntick = 8\nmove = [1, 1]\n"),
            OrderScriptError::Unordered { tick: 8 }
        );
        assert_eq!(
            flaw("[[order]]\ntick = 9\nmove = [99999999999999, 1]\n"),
            OrderScriptError::Coordinate { tick: 9 }
        );
        assert!(matches!(
            flaw("[[order]]\ntick = 9\nwalk = [1, 1]\n"),
            OrderScriptError::Toml(_)
        ));
        // A cast of slot 2 with no target, one of slot 0 at unit 7, and an attack on unit 3; an
        // order of no action, of two, or of a target with no cast is refused.
        let actions = "[[order]]\ntick = 9\ncast = 2\n[[order]]\ntick = 9\ncast = 0\ntarget = 7\n\
                       [[order]]\ntick = 10\nattack = 3\n";
        let orders = OrderScript::parse(actions).unwrap();
        let [first, second, third] = orders.orders() else {
            panic!("three orders");
        };
        assert_eq!(
            (first.tick, first.action),
            (
                Tick::new(9),
                Action::Slot {
                    slot: 2,
                    target: ActionTarget::None
                }
            )
        );
        assert!(matches!(
            (second.tick.get(), second.action),
            (9, Action::Slot { slot: 0, target: ActionTarget::Unit(unit) }) if unit.get() == 7
        ));
        assert!(matches!(
            (third.tick.get(), third.action),
            (10, Action::Attack { target }) if target.get() == 3
        ));
        for flawed in [
            "[[order]]\ntick = 9\n",
            "[[order]]\ntick = 9\ncast = 0\nmove = [1, 1]\n",
            "[[order]]\ntick = 9\nattack = 3\ncast = 0\n",
            "[[order]]\ntick = 9\ntarget = 3\n",
            "[[order]]\ntick = 9\nattack = 3\ntarget = 4\n",
        ] {
            assert_eq!(
                flaw(flawed),
                OrderScriptError::Action { tick: 9 },
                "{flawed}"
            );
        }
    }
}
