use bevy_ecs::resource::Resource;
use campfire_common::Tick;

use crate::order_script::{OrderScript, ScriptedOrder};

/// A script a client plays for its player: each order goes out, for the player's own avatar, in
/// the tick of its stamp, with the orders the player gives by hand.
#[derive(Resource, Debug)]
pub struct BotScript {
    script: OrderScript,
    /// The first order not sent yet.
    next: usize,
}

impl BotScript {
    pub const fn new(script: OrderScript) -> BotScript {
        BotScript { script, next: 0 }
    }

    /// Takes the orders due by `tick`, each once: a tick that passed while the match did not
    /// run sends its orders late.
    pub(crate) fn due(&mut self, tick: Tick) -> &[ScriptedOrder] {
        let orders = self.script.orders();
        let start = self.next;
        self.next += orders[start..].partition_point(|order| order.tick <= tick);
        &orders[start..self.next]
    }
}
