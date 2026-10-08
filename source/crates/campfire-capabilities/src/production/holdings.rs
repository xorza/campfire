use campfire_common::PlayerSlot;

use crate::production::held::Held;
use crate::production::requirements::Required;
use crate::stats::player_modifiers::PlayerModifiers;

/// What the players hold as a tick's trains and builds start: their living, complete units, by
/// type, each once, in order, and their modifiers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Holdings<'a> {
    pub(crate) units: &'a [Held],
    pub(crate) modifiers: Option<&'a PlayerModifiers>,
}

impl Holdings<'_> {
    /// Whether `owner` holds what `required` needs: one with no `requires` needs nothing, and a
    /// unit no player owns meets no need.
    pub(crate) fn meet(self, owner: Option<PlayerSlot>, required: Option<Required<'_>>) -> bool {
        let Some(required) = required else {
            return true;
        };
        let Some(owner) = owner else {
            return false;
        };
        let units = required
            .units
            .iter()
            .all(|&unit_type| self.units.binary_search(&Held { owner, unit_type }).is_ok());
        let modifiers = required.modifiers.iter().all(|&modifier| {
            self.modifiers
                .is_some_and(|held| held.holds(owner, modifier))
        });
        units && modifiers
    }
}
