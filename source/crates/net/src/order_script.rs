use campfire_capabilities::{Action, Scalar};
use campfire_sim::Tick;
use serde::Deserialize;

use crate::error::OrderScriptError;

/// A player's orders for their hero, each at a sim tick, in tick order: what a bot plays. It
/// reads from TOML, one `[[order]]` table each, with its `tick` and `move = [x, z]` in meters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderScript {
    orders: Vec<ScriptedOrder>,
}

/// One order of a script: the sim tick it is sent in, and what the hero does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScriptedOrder {
    pub(crate) tick: Tick,
    pub(crate) action: Action,
}

impl OrderScript {
    /// The script `text` holds; an error when it is not TOML of this shape, when a coordinate is
    /// past what a sim number holds, or when the ticks do not grow.
    pub fn parse(text: &str) -> Result<OrderScript, OrderScriptError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default)]
            order: Vec<Entry>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            tick: u64,
            #[serde(rename = "move")]
            to: [Scalar; 2],
        }
        let file: File = toml::from_str(text).map_err(OrderScriptError::Toml)?;
        let mut orders = Vec::with_capacity(file.order.len());
        for Entry { tick, to } in file.order {
            let [x, z] = to.map(Scalar::to_num);
            let (Some(x), Some(z)) = (x, z) else {
                return Err(OrderScriptError::Coordinate { tick });
            };
            let tick = Tick::new(tick);
            if orders
                .last()
                .is_some_and(|last: &ScriptedOrder| last.tick > tick)
            {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            orders.push(ScriptedOrder {
                tick,
                action: Action::Move { x, z },
            });
        }
        Ok(OrderScript { orders })
    }

    pub(crate) fn orders(&self) -> &[ScriptedOrder] {
        &self.orders
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
        assert_eq!(OrderScript::parse("").unwrap().orders(), []);
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
    }
}
