use crate::actions::slot_kind::SlotKind;
use crate::units::action_id::ActionId;
use crate::values::rank::Rank;

/// An action in a slot kind of a unit type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotAction {
    pub(crate) kind: SlotKind,
    pub(crate) ability: ActionId,
    /// Its rank as its unit spawns: its kind's first.
    pub(crate) rank: Option<Rank>,
}
