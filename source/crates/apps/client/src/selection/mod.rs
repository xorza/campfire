use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::SystemSet;
use campfire_capabilities::{Action, Order};
use campfire_players::OrderUnits;
use campfire_sim::StableId;

/// The player's own units that a right click and the stop key order, in stable id order, each
/// once (design 17, D7). A click or a drag that meets none of the player's units leaves it as it
/// is; a unit the client no longer holds leaves it, and an empty selection takes the avatar, so a
/// MOBA's player orders their avatar as before.
#[derive(Resource, Debug, Default)]
pub(crate) struct Selection(Vec<StableId>);

/// The systems that change the selection, which the orders and the selection's rings read after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SelectionSystems;

impl Selection {
    pub(crate) fn get(&self) -> &[StableId] {
        &self.0
    }

    /// Selects `units` alone, of any order, a unit there more than once selected once; none
    /// leaves the selection as it is. Whether it changed.
    pub(crate) fn pick(&mut self, units: &[StableId]) -> bool {
        if units.is_empty() {
            return false;
        }
        self.0.clear();
        self.0.extend_from_slice(units);
        self.0.sort_unstable();
        self.0.dedup();
        true
    }

    /// Drops each unit that `held` says the client no longer holds, then, when none is left,
    /// takes `avatar`. Whether it changed.
    pub(crate) fn keep(
        &mut self,
        held: impl Fn(StableId) -> bool,
        avatar: Option<StableId>,
    ) -> bool {
        let before = self.0.len();
        self.0.retain(|&unit| held(unit));
        let dropped = self.0.len() != before;
        match avatar {
            Some(avatar) if self.0.is_empty() => {
                self.0.push(avatar);
                true
            }
            _ => dropped,
        }
    }

    /// The order of `action` to every selected unit, as one group order; none for an empty
    /// selection.
    pub(crate) fn order(&self, action: impl Into<Action>) -> Option<Order> {
        Some(Order {
            units: OrderUnits::new(&self.0)?,
            action: action.into(),
        })
    }
}

#[cfg(test)]
mod tests;
