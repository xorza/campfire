use campfire_common::PlayerSlot;

use crate::scripts::call_start::{CallStart, Purity};
use crate::scripts::script_role::ScriptRole;

/// The pool a script call draws from, as scripting gives them (02-engine-core.md, Operation pools);
/// its `systems` pool, of `on_system`, is planned, and `calc_damage`, `calc_heal` and `on_generate`
/// draw from none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
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

impl Pool {
    /// The pool the call `start` draws from, none for a pure call: an AI's think, the think pool;
    /// a call of the mode, the mode's, or its payer's when it answers a player's input; an
    /// action's or a modifier's, its payer's, the player that controls its acting unit, the think
    /// pool when none does or the unit is gone, and the mode's when it has no acting unit, as a
    /// modifier from no source.
    pub const fn of_call(start: &CallStart) -> Option<Self> {
        if matches!(start.purity, Purity::Reads) {
            return None;
        }
        Some(match (start.role, start.payer, start.acting) {
            (ScriptRole::Ai, _, _) => Self::Think,
            (_, Some(slot), _) => Self::Player(slot),
            (ScriptRole::Mode, None, _)
            | (ScriptRole::Action | ScriptRole::Modifier, None, None) => Self::Mode,
            (ScriptRole::Action | ScriptRole::Modifier, None, Some(_)) => Self::Think,
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Tick;
    use campfire_sim::IdAllocator;
    use campfire_values::{ActionId, ModifierId, PackageIndex, Rank};

    use super::*;
    use crate::scripts::call_start::Spent;
    use crate::scripts::mode_scripts::ListPlace;

    #[test]
    fn a_call_draws_from_its_payers_pool_and_only_a_call_no_player_causes_waits() {
        let now = Tick::new(3);
        let unit = IdAllocator::default().allocate();
        let slot = PlayerSlot::new(1);
        let package = PackageIndex::MODE;
        let cast = CallStart::cast(ActionId::new(0), Rank::FIRST, unit, package, now);
        let hook = CallStart::hook(ModifierId::new(0), package, 0, now);
        let mode = CallStart::mode(ScriptRole::Mode, now);
        let paid = |start: CallStart| CallStart {
            payer: Some(slot),
            ..start
        };
        let sourced = CallStart {
            acting: Some(unit),
            ..hook
        };
        let cases = [
            // A cast by a player's unit, and one by a unit no player controls or that is gone.
            (paid(cast), Some(Pool::Player(slot)), Spent::Fails),
            (cast, Some(Pool::Think), Spent::Fails),
            // A modifier's hook from a player's unit, from another unit, and from no source.
            (paid(sourced), Some(Pool::Player(slot)), Spent::Fails),
            (sourced, Some(Pool::Think), Spent::Fails),
            (hook, Some(Pool::Mode), Spent::Fails),
            // The mode's event, and its answer to a player's input.
            (mode, Some(Pool::Mode), Spent::Waits),
            (paid(mode), Some(Pool::Player(slot)), Spent::Fails),
            // A think, of any unit's, and a pure hook, which draws from none.
            (CallStart::think(unit, now), Some(Pool::Think), Spent::Waits),
            (
                paid(CallStart::think(unit, now)),
                Some(Pool::Think),
                Spent::Waits,
            ),
            (CallStart::pure(ListPlace::FIRST, now), None, Spent::Waits),
        ];
        for (start, pool, spent) in cases {
            assert_eq!(
                (Pool::of_call(&start), start.when_spent()),
                (pool, spent),
                "{start:?}"
            );
        }
    }
}
