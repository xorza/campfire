use bevy_ecs::resource::Resource;

use crate::script_host::budget::Budget;
use crate::scripts::pool::Pool;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_pools::ScriptPools;

/// What each pool has left in the running tick. Not state: every tick starts with the full
/// limits, and a call that ran out changed nothing.
#[derive(Resource, Debug, Clone)]
pub struct ScriptBudgets {
    limits: ScriptLimits,
    /// By seat.
    players: Vec<Budget>,
    think: Budget,
    mode: Budget,
}

impl ScriptBudgets {
    /// Full budgets of `pools`.
    pub fn full(pools: ScriptPools) -> Self {
        let ScriptPools { limits, players } = pools;
        let players = usize::try_from(players).expect("a slot count fits usize");
        Self {
            limits,
            players: vec![Budget::new(limits.player); players],
            think: Budget::new(limits.think),
            mode: Budget::new(limits.mode),
        }
    }

    /// Fills every pool again, as a tick starts.
    pub fn refill(&mut self) {
        self.players.fill(Budget::new(self.limits.player));
        self.think = Budget::new(self.limits.think);
        self.mode = Budget::new(self.limits.mode);
    }

    /// The budget of `pool`. A player's is only for a slot of the match.
    pub fn get_mut(&mut self, pool: Pool) -> &mut Budget {
        match pool {
            Pool::Player(slot) => &mut self.players[slot.index()],
            Pool::Think => &mut self.think,
            Pool::Mode => &mut self.mode,
        }
    }
}
