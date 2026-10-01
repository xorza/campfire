use std::rc::Rc;

use crate::scripts::script_limits::ScriptLimits;
use crate::stats::stat::Stat;
use crate::values::declared_name::DeclaredName;

/// The scripts a match runs: within `limits`, with a pool for each of its `players` slots,
/// dealing the `[combat] damage_kinds` the mode declares, and naming its `stats`, its `pools` and
/// its players' `resources`, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchScripts {
    pub limits: ScriptLimits,
    pub players: u32,
    pub damage_kinds: Rc<[DeclaredName]>,
    pub stats: Rc<[Stat]>,
    pub pools: Rc<[DeclaredName]>,
    pub resources: Rc<[DeclaredName]>,
}

#[cfg(test)]
pub(crate) mod internals {
    use std::rc::Rc;

    use crate::scripts::match_scripts::MatchScripts;
    use crate::scripts::script_limits::ScriptLimits;

    /// The scripts of a match of `players` players within `limits` that names no damage kind,
    /// stat, pool or player resource.
    pub(crate) fn bare(limits: ScriptLimits, players: u32) -> MatchScripts {
        MatchScripts {
            limits,
            players,
            damage_kinds: Rc::from([]),
            stats: Rc::from([]),
            pools: Rc::from([]),
            resources: Rc::from([]),
        }
    }
}
