use bevy_ecs::resource::Resource;
use campfire_script::Budget;

use crate::units::pool::Pool;
use crate::units::script_limits::ScriptLimits;

/// What each pool has left in the running tick. Not state: every tick starts with the full
/// limits, and a call that ran out changed nothing.
#[derive(Resource, Debug)]
pub(crate) struct ScriptBudgets {
    limits: ScriptLimits,
    /// By player slot.
    players: Vec<Budget>,
    think: Budget,
    mode: Budget,
}

impl ScriptBudgets {
    pub(crate) fn new(limits: ScriptLimits, players: u32) -> ScriptBudgets {
        let players = usize::try_from(players).expect("a slot count fits usize");
        ScriptBudgets {
            limits,
            players: vec![Budget::new(limits.player); players],
            think: Budget::new(limits.think),
            mode: Budget::new(limits.mode),
        }
    }

    pub(crate) fn begin_tick(&mut self) {
        self.players.fill(Budget::new(self.limits.player));
        self.think = Budget::new(self.limits.think);
        self.mode = Budget::new(self.limits.mode);
    }

    /// The budget of `pool`. A player's is only for a slot of the match.
    pub(crate) fn get_mut(&mut self, pool: Pool) -> &mut Budget {
        match pool {
            Pool::Player(slot) => &mut self.players[slot as usize],
            Pool::Think => &mut self.think,
            Pool::Mode => &mut self.mode,
        }
    }
}
