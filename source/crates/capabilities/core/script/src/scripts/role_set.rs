use crate::scripts::script_role::ScriptRole;

/// The script roles a name of the script API serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleSet(u8);

impl RoleSet {
    pub const ALL: Self = Self::of(&ScriptRole::ALL);
    /// The roles of the sim's calls: every one but a bot's, whose calls run outside it.
    pub const SIM: Self = Self::of(&[
        ScriptRole::Action,
        ScriptRole::Modifier,
        ScriptRole::Mode,
        ScriptRole::Ai,
    ]);
    /// The roles whose calls have an acting unit: every one but the mode's.
    pub const ACTING: Self = Self::of(&[ScriptRole::Action, ScriptRole::Modifier, ScriptRole::Ai]);
    /// The role of an action's own calls, not its modifiers'.
    pub const ACTION: Self = Self::of(&[ScriptRole::Action]);
    pub const MODE: Self = Self::of(&[ScriptRole::Mode]);
    pub const AI: Self = Self::of(&[ScriptRole::Ai]);
    pub const BOT: Self = Self::of(&[ScriptRole::Bot]);

    pub const fn of(roles: &[ScriptRole]) -> Self {
        let mut bits = 0;
        let mut at = 0;
        while at < roles.len() {
            bits |= 1 << roles[at] as u8;
            at += 1;
        }
        Self(bits)
    }

    /// The roles of both.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
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
