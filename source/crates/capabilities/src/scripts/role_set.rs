use crate::scripts::hook::ScriptRole;

/// The script roles a name of the script API serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleSet(u8);

impl RoleSet {
    pub const ALL: RoleSet = RoleSet::of(&ScriptRole::ALL);
    /// The roles whose calls have an acting unit: every one but the mode's.
    pub const ACTING: RoleSet =
        RoleSet::of(&[ScriptRole::Action, ScriptRole::Modifier, ScriptRole::Ai]);
    /// The role of an action's own calls, not its modifiers'.
    pub const ACTION: RoleSet = RoleSet::of(&[ScriptRole::Action]);
    pub const MODE: RoleSet = RoleSet::of(&[ScriptRole::Mode]);
    pub const AI: RoleSet = RoleSet::of(&[ScriptRole::Ai]);

    pub const fn of(roles: &[ScriptRole]) -> RoleSet {
        let mut bits = 0;
        let mut at = 0;
        while at < roles.len() {
            bits |= 1 << roles[at] as u8;
            at += 1;
        }
        RoleSet(bits)
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
