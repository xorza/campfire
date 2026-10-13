use bevy_ecs::resource::Resource;
use campfire_script::Budget;

use crate::scripts::pool::Pool;
use crate::scripts::script_limits::ScriptLimits;

/// What each pool has left in the running tick. Not state: every tick starts with the full
/// limits, and a call that ran out changed nothing.
#[derive(Resource, Debug, Clone)]
pub struct ScriptBudgets {
    limits: ScriptLimits,
    /// By player slot.
    players: Vec<Budget>,
    think: Budget,
    mode: Budget,
}

impl ScriptBudgets {
    /// Full budgets within `limits`, one for each of `players` player slots.
    pub fn new(limits: ScriptLimits, players: u32) -> ScriptBudgets {
        let players = usize::try_from(players).expect("a slot count fits usize");
        ScriptBudgets {
            limits,
            players: vec![Budget::new(limits.player); players],
            think: Budget::new(limits.think),
            mode: Budget::new(limits.mode),
        }
    }

    /// The limits it starts each tick with.
    pub(crate) const fn limits(&self) -> ScriptLimits {
        self.limits
    }

    pub(crate) fn begin_tick(&mut self) {
        self.players.fill(Budget::new(self.limits.player));
        self.think = Budget::new(self.limits.think);
        self.mode = Budget::new(self.limits.mode);
    }

    /// The budget of `pool`. A player's is only for a slot of the match.
    pub(crate) fn get_mut(&mut self, pool: Pool) -> &mut Budget {
        match pool {
            Pool::Player(slot) => &mut self.players[slot.index()],
            Pool::Think => &mut self.think,
            Pool::Mode => &mut self.mode,
        }
    }
}
