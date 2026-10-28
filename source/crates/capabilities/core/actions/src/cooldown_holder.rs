use serde::{Deserialize, Serialize};

/// Who keeps an action's cooldown: its unit, in its slot, or its unit's player, shared by every
/// unit of the player that holds the action (04-capabilities/actions.md, A power's cooldown may be
/// its player's).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooldownHolder {
    #[default]
    Unit,
    Player,
}

impl CooldownHolder {
    /// Whether it is the unit, which data leaves out.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's test of a field to skip takes it by reference"
    )]
    pub(crate) const fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }
}
