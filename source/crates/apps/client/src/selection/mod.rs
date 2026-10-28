use bevy::ecs::message::Message;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::SystemSet;
use campfire_capabilities::{Directive, Order};
use campfire_players::OrderUnits;
use campfire_sim::StableId;

/// The player's own units that a right click and the stop key order, in stable id order, each once
/// (02-engine-core.md, A selection of the player's own units). A click or a drag that meets none of
/// the player's units leaves it as it is; a unit the client no longer holds leaves it, and an empty
/// selection takes the avatar, so a MOBA's player orders their avatar as before.
#[derive(Resource, Debug, Default)]
pub(crate) struct Selection(Vec<StableId>);

/// What the player's pick of a selection, or an order to it, asks it to answer with, as the voice
/// of its first unit of one (02-engine-core.md, A look names its voices): the selection's own pick
/// asks it, as Zero Hour answers a selection the player makes, never one a death or a despawn
/// changes.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionVoice {
    Select,
    Move,
    Attack,
}

/// The systems that change the selection, which the orders and the selection's rings read after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SelectionSystems;

impl Selection {
    pub(crate) fn get(&self) -> &[StableId] {
        &self.0
    }

    /// Selects `units` alone, of any order, each spawn that `led_by` gives a leader of as that
    /// leader, as Zero Hour shares one selection between a mob and its members
    /// (02-engine-core.md, A member picks as its leader), a unit there more than once selected
    /// once; none leaves the selection as it is. Whether it changed.
    pub(crate) fn pick(
        &mut self,
        units: &[StableId],
        led_by: impl Fn(StableId) -> Option<StableId>,
    ) -> bool {
        if units.is_empty() {
            return false;
        }
        self.0.clear();
        let picked = units.iter().map(|&unit| led_by(unit).unwrap_or(unit));
        self.0.extend(picked);
        self.0.sort_unstable();
        self.0.dedup();
        true
    }

    /// Adds `units`, as `pick` takes them, to the units it holds, as Zero Hour's prefer-selection
    /// key adds them (02-engine-core.md, Shift adds, a double click takes a kind);
    /// whether it changed.
    pub(crate) fn add(
        &mut self,
        units: &[StableId],
        led_by: impl Fn(StableId) -> Option<StableId>,
    ) -> bool {
        let before = self.0.len();
        self.0
            .extend(units.iter().map(|&unit| led_by(unit).unwrap_or(unit)));
        self.0.sort_unstable();
        self.0.dedup();
        self.0.len() != before
    }

    /// Takes `units`, as `pick` takes them, out of it when it holds each of them, as Zero Hour's
    /// prefer-selection key takes a click's units all selected; whether it did.
    pub(crate) fn remove(
        &mut self,
        units: &[StableId],
        led_by: impl Fn(StableId) -> Option<StableId>,
    ) -> bool {
        let led = |unit: &StableId| led_by(*unit).unwrap_or(*unit);
        let held = !units.is_empty()
            && units
                .iter()
                .all(|unit| self.0.binary_search(&led(unit)).is_ok());
        if held {
            self.0
                .retain(|unit| !units.iter().any(|picked| led(picked) == *unit));
        }
        held
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

    /// The order of `directive` to every selected unit, as one group order; none for an empty
    /// selection.
    pub(crate) fn order(&self, directive: impl Into<Directive>) -> Option<Order> {
        Some(Order {
            units: OrderUnits::new(&self.0)?,
            directive: directive.into(),
        })
    }
}

#[cfg(test)]
mod tests;
