use campfire_sim::StableId;

use crate::scripts::script_role::ScriptRole;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
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

    /// A cast of `action` of `package` at `rank` by `caster`, for no hit.
    pub(crate) const fn cast(
        action: ActionId,
        rank: u8,
        caster: StableId,
        package: u16,
    ) -> CallStart {
        CallStart {
            role: ScriptRole::Action,
            acting: Some(caster),
            action: Some(action),
            rank,
            modifier: None,
            package,
            depth: 0,
            hit: None,
        }
    }

    /// A hook of `modifier` of `package` at chain depth `depth`, of an instance from no source,
    /// by no action, at rank 1.
    pub(crate) const fn hook(modifier: ModifierId, package: u16, depth: u8) -> CallStart {
        CallStart {
            role: ScriptRole::Modifier,
            acting: None,
            action: None,
            rank: 1,
            modifier: Some(modifier),
            package,
            depth,
            hit: None,
        }
    }
}
