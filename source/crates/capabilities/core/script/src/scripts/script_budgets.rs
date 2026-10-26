use bevy_ecs::resource::Resource;

use crate::script_host::budget::Budget;
use crate::scripts::budget_of::BudgetOf;
use crate::scripts::budget_setup::BudgetSetup;
use crate::scripts::script_limits::ScriptLimits;

/// What each budget has left in the running tick. Not state: every tick starts with the full
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
    /// Full budgets of `setup`.
    pub fn full(setup: BudgetSetup) -> Self {
        let BudgetSetup { limits, players } = setup;
        let players = usize::try_from(players).expect("a slot count fits usize");
        Self {
            limits,
            players: vec![Budget::new(limits.player); players],
            think: Budget::new(limits.think),
            mode: Budget::new(limits.mode),
        }
    }

    /// Fills every budget again, as a tick starts.
    pub fn refill(&mut self) {
        self.players.fill(Budget::new(self.limits.player));
        self.think = Budget::new(self.limits.think);
        self.mode = Budget::new(self.limits.mode);
    }

    /// The budget `of` names. A player's is only for a slot of the match.
    pub fn get_mut(&mut self, of: BudgetOf) -> &mut Budget {
        match of {
            BudgetOf::Player(slot) => &mut self.players[slot.index()],
            BudgetOf::Think => &mut self.think,
            BudgetOf::Mode => &mut self.mode,
        }
    }
}
