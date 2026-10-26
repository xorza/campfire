use campfire_common::PlayerSlot;

use crate::scripts::call_start::{CallStart, Purity};
use crate::scripts::script_role::ScriptRole;

/// The budget a script call draws from, as scripting gives them (02-engine-core.md, Operation
/// budgets); its `systems` budget, of `on_system`, is planned, and `calc_damage`, `calc_heal` and
/// `on_generate` draw from none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetOf {
    /// The calls a player causes: their units' actions, with those actions' deliveries and
    /// modifiers, and their mode inputs.
    Player(PlayerSlot),
    /// AI `on_think` calls, and an action's or a modifier's hooks whose source no player
    /// controls, or is gone.
    Think,
    /// The mode's own hooks: the match start, timers, players' joins and leaves, deaths and
    /// `on_level_up`; and a modifier's hooks when it has no source.
    Mode,
}

impl BudgetOf {
    /// The budget the call `start` draws from, none for a pure call: an AI's think, the think
    /// budget; a call of the mode, the mode's, or its payer's when it answers a player's input; an
    /// action's or a modifier's, its payer's, the player that controls its acting unit, the think
    /// budget when none does or the unit is gone, and the mode's when it has no acting unit, as a
    /// modifier from no source.
    pub const fn of_call(start: &CallStart) -> Option<Self> {
        if matches!(start.purity, Purity::Reads) {
            return None;
        }
        Some(match (start.role, start.payer, start.acting) {
            // A bot's call reads, from the bot driver's own budget.
            (ScriptRole::Bot, _, _) => return None,
            (ScriptRole::Ai, _, _) => Self::Think,
            (_, Some(slot), _) => Self::Player(slot),
            (ScriptRole::Mode, None, _)
            | (ScriptRole::Action | ScriptRole::Modifier, None, None) => Self::Mode,
            (ScriptRole::Action | ScriptRole::Modifier, None, Some(_)) => Self::Think,
        })
    }
}

#[cfg(test)]
mod tests;
