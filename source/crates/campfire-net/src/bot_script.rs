use bevy_ecs::resource::Resource;
use campfire_common::Tick;

use crate::order_script::{OrderScript, ScriptedInput, ScriptedOrder};

/// A script a bot plays for its player: each order goes out, for the player's own avatar, and
/// each mode input, in the tick of its stamp, with the orders the player gives by hand.
#[derive(Resource, Debug)]
pub struct BotScript {
    script: OrderScript,
    /// The first order not sent yet.
    next_order: usize,
    /// The first input not sent yet.
    next_input: usize,
}

impl BotScript {
    pub const fn new(script: OrderScript) -> BotScript {
        BotScript {
            script,
            next_order: 0,
            next_input: 0,
        }
    }

    /// Takes the mode inputs due by `tick`, each once: a tick that passed while the match did not
    /// run sends its own late.
    pub(crate) fn due_inputs(&mut self, tick: Tick) -> &[ScriptedInput] {
        let inputs = self.script.inputs();
        let first = self.next_input;
        self.next_input += inputs[first..].partition_point(|input| input.tick <= tick);
        &inputs[first..self.next_input]
    }

    /// Takes the orders due by `tick`, each once, as `due_inputs` takes the mode inputs.
    pub(crate) fn due_orders(&mut self, tick: Tick) -> &[ScriptedOrder] {
        let orders = self.script.orders();
        let first = self.next_order;
        self.next_order += orders[first..].partition_point(|order| order.tick <= tick);
        &orders[first..self.next_order]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bot_takes_its_inputs_and_its_orders_apart_each_once() {
        let script = OrderScript::parse(
            "[[input]]\ntick = 0\nname = \"hero\"\nvalue = \"hero-x\"\n\
             [[order]]\ntick = 2\nmove = [1, 0]\n[[order]]\ntick = 5\nmove = [0, 1]\n",
        )
        .unwrap();
        let ticks = |orders: &[ScriptedOrder]| -> Vec<u64> {
            orders.iter().map(|order| order.tick.get()).collect()
        };
        let mut bot = BotScript::new(script);
        // A client with no avatar yet takes the inputs only: the orders due by tick 3 wait.
        assert_eq!(bot.due_inputs(Tick::new(3)).len(), 1);
        assert_eq!(bot.due_inputs(Tick::new(3)), []);
        // Taken late, at tick 6, the orders due by then go out together, each once.
        assert_eq!(ticks(bot.due_orders(Tick::new(6))), [2, 5]);
        assert!(bot.due_orders(Tick::new(9)).is_empty());
    }
}
