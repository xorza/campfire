use campfire_sim::StableId;

use crate::scripts::hook::ScriptRole;
use crate::stats::modifier_book::ModifierId;
use crate::units::action_id::ActionId;
use crate::values::hit::Hit;

/// What a call starts with: its role; its acting unit, a cast's caster, a modifier's source or
/// the unit that thinks, none for the mode; the action whose params it reads, at `rank`, and its
/// modifier's; the package whose names it means, 0 the mode's; the depth of the chain of combat
/// events it runs in, 0 outside one; and the hit a delivery's hook runs for, which the damage
/// it deals carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallStart {
    pub(crate) role: ScriptRole,
    pub(crate) acting: Option<StableId>,
    pub(crate) action: Option<ActionId>,
    pub(crate) rank: u8,
    pub(crate) modifier: Option<ModifierId>,
    pub(crate) package: u16,
    pub(crate) depth: u8,
    pub(crate) hit: Option<Hit>,
}

impl CallStart {
    /// A call of `role` with no acting unit, action, modifier or chain, of the mode's package, at
    /// rank 1.
    pub(crate) const fn mode(role: ScriptRole) -> CallStart {
        CallStart {
            role,
            acting: None,
            action: None,
            rank: 1,
            modifier: None,
            package: 0,
            depth: 0,
            hit: None,
        }
    }
}
