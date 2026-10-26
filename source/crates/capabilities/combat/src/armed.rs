use campfire_actions::{ActionBook, ActionSlots, Targets};
use campfire_script::LivingUnit;
use campfire_values::{TagSet, Team};

/// What a unit attacks with: its slots, of the action book, its team, and its tags.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Armed<'a> {
    pub(crate) slots: &'a ActionSlots,
    pub(crate) book: &'a ActionBook,
    pub(crate) team: Team,
    pub(crate) holder: TagSet,
}

impl Armed<'_> {
    /// The slot of the weapon it attacks `target` with, as it regards the target by `targets`:
    /// the first whose `when` its tags pass and whose filter selects the target; none for no such
    /// weapon. Its attack starts with it, and its turret aims for it, so the two never differ.
    pub(crate) fn weapon_against(
        self,
        targets: &Targets<'_, '_>,
        target: &LivingUnit,
    ) -> Option<u8> {
        let selects = (targets.relation(self.team, target.team), target.tags);
        self.slots.weapon_in(self.book, self.holder, Some(selects))
    }
}
