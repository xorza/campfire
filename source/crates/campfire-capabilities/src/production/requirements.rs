use std::ops::Range;

use bevy_ecs::resource::Resource;

use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;

/// What each train and build with a `requires` needs its player to hold, by action: unit types of
/// which it owns a living, complete unit, and player modifiers, one run of each after another.
/// Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Requirements {
    /// By action, as the book loads them in order.
    needs: Vec<Needs>,
    units: Vec<UnitType>,
    modifiers: Vec<ModifierId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Needs {
    action: ActionId,
    units: Range<u32>,
    modifiers: Range<u32>,
}

/// What one train or build needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Required<'a> {
    pub(crate) units: &'a [UnitType],
    pub(crate) modifiers: &'a [ModifierId],
}

impl Requirements {
    /// Gives `action`, loaded after every action it holds, what it needs.
    pub(crate) fn push(
        &mut self,
        action: ActionId,
        units: impl IntoIterator<Item = UnitType>,
        modifiers: impl IntoIterator<Item = ModifierId>,
    ) {
        debug_assert!(self.needs.last().is_none_or(|last| last.action < action));
        let run = |at: usize| u32::try_from(at).expect("a book's requirements fit a u32");
        let unit_start = self.units.len();
        self.units.extend(units);
        let modifier_start = self.modifiers.len();
        self.modifiers.extend(modifiers);
        self.needs.push(Needs {
            action,
            units: run(unit_start)..run(self.units.len()),
            modifiers: run(modifier_start)..run(self.modifiers.len()),
        });
    }

    /// What `action` needs; `None` for one with no `requires`.
    pub(crate) fn of(&self, action: ActionId) -> Option<Required<'_>> {
        let at = self
            .needs
            .binary_search_by_key(&action, |needs| needs.action)
            .ok()?;
        let needs = &self.needs[at];
        let range = |run: &Range<u32>| run.start as usize..run.end as usize;
        Some(Required {
            units: &self.units[range(&needs.units)],
            modifiers: &self.modifiers[range(&needs.modifiers)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_finds_its_own_runs_and_one_with_none_finds_nothing() {
        let mut requirements = Requirements::default();
        requirements.push(ActionId::new(1), [UnitType::new(4)], []);
        requirements.push(
            ActionId::new(3),
            [UnitType::new(2), UnitType::new(5)],
            [ModifierId::new(0)],
        );
        let of = |at| requirements.of(ActionId::new(at));
        assert_eq!(
            of(1),
            Some(Required {
                units: &[UnitType::new(4)],
                modifiers: &[],
            })
        );
        assert_eq!(
            of(3),
            Some(Required {
                units: &[UnitType::new(2), UnitType::new(5)],
                modifiers: &[ModifierId::new(0)],
            })
        );
        assert_eq!([of(0), of(2), of(4)], [None; 3]);
    }
}
