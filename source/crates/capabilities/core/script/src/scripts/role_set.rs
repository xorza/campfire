use crate::scripts::script_role::ScriptRole;

/// The script roles a name of the script API serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleSet(u8);

impl RoleSet {
    pub const ALL: Self = Self::of(&ScriptRole::ALL);
    /// The roles whose calls have an acting unit: every one but the mode's.
    pub const ACTING: Self = Self::of(&[ScriptRole::Action, ScriptRole::Modifier, ScriptRole::Ai]);
    /// The role of an action's own calls, not its modifiers'.
    pub const ACTION: Self = Self::of(&[ScriptRole::Action]);
    pub const MODE: Self = Self::of(&[ScriptRole::Mode]);
    pub const AI: Self = Self::of(&[ScriptRole::Ai]);

    pub const fn of(roles: &[ScriptRole]) -> Self {
        let mut bits = 0;
        let mut at = 0;
        while at < roles.len() {
            bits |= 1 << roles[at] as u8;
            at += 1;
        }
        Self(bits)
    }

    pub const fn contains(self, role: ScriptRole) -> bool {
        self.0 & (1 << role as u8) != 0
    }

    pub fn iter(self) -> impl Iterator<Item = ScriptRole> {
        ScriptRole::ALL
            .into_iter()
            .filter(move |&role| self.contains(role))
    }
}
