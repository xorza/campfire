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

/// What a script has due by a tick, each once.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Due<'a> {
    pub(crate) inputs: &'a [ScriptedInput],
    pub(crate) orders: &'a [ScriptedOrder],
}

impl BotScript {
    pub const fn new(script: OrderScript) -> BotScript {
        BotScript {
            script,
            next_order: 0,
            next_input: 0,
        }
    }

    /// Takes the orders and inputs due by `tick`, each once: a tick that passed while the match
    /// did not run sends its own late.
    pub(crate) fn due(&mut self, tick: Tick) -> Due<'_> {
        let orders = self.script.orders();
        let start = self.next_order;
        self.next_order += orders[start..].partition_point(|order| order.tick <= tick);
        let inputs = self.script.inputs();
        let first = self.next_input;
        self.next_input += inputs[first..].partition_point(|input| input.tick <= tick);
        Due {
            inputs: &inputs[first..self.next_input],
            orders: &orders[start..self.next_order],
        }
    }
}
