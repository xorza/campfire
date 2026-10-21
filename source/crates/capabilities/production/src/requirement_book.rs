use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::MatchNames;
use campfire_values::{ActionId, ModifierId, UnitType};

/// What each train and build with a `requires` needs its player to hold, by action: entries of
/// unit types, of one of which it owns a living, complete unit, and player modifiers, one run of
/// each after another, each entry a run of its types (12-base-defenses.md, A requirement may be
/// any one of several types). Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct RequirementBook {
    /// By action, as the book loads them in order.
    needs: Vec<Needs>,
    /// Each entry's run of `types`.
    entries: Vec<Range<u32>>,
    types: Vec<UnitType>,
    modifiers: Vec<ModifierId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Needs {
    action: ActionId,
    entries: Range<u32>,
    modifiers: Range<u32>,
}

/// What one train or build needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Required<'a> {
    entries: &'a [Range<u32>],
    types: &'a [UnitType],
    pub modifiers: &'a [ModifierId],
}

impl RequirementBook {
    /// Gives `action`, loaded after every action it holds, what it needs: `units`, entries of
    /// one type at least each, and `modifiers`.
    pub fn push<E: IntoIterator<Item = UnitType>>(
        &mut self,
        action: ActionId,
        units: impl IntoIterator<Item = E>,
        modifiers: impl IntoIterator<Item = ModifierId>,
    ) {
        debug_assert!(self.needs.last().is_none_or(|last| last.action < action));
        let run = |at: usize| u32::try_from(at).expect("a book's requirements fit a u32");
        let entry_start = self.entries.len();
        for entry in units {
            let start = self.types.len();
            self.types.extend(entry);
            debug_assert!(self.types.len() > start, "an entry holds one type at least");
            self.entries.push(run(start)..run(self.types.len()));
        }
        let modifier_start = self.modifiers.len();
        self.modifiers.extend(modifiers);
        self.needs.push(Needs {
            action,
            entries: run(entry_start)..run(self.entries.len()),
            modifiers: run(modifier_start)..run(self.modifiers.len()),
        });
    }

    /// What `action` needs; `None` for one with no `requires`.
    pub fn of(&self, action: ActionId) -> Option<Required<'_>> {
        let at = self
            .needs
            .binary_search_by_key(&action, |needs| needs.action)
            .ok()?;
        let needs = &self.needs[at];
        let range = |run: &Range<u32>| run.start as usize..run.end as usize;
        Some(Required {
            entries: &self.entries[range(&needs.entries)],
            types: &self.types,
            modifiers: &self.modifiers[range(&needs.modifiers)],
        })
    }
}

impl<'a> Required<'a> {
    /// Its entries, each the unit types any one of which meets it.
    pub fn units(&self) -> impl ExactSizeIterator<Item = &'a [UnitType]> + use<'a> {
        let types = self.types;
        self.entries
            .iter()
            .map(move |entry| &types[entry.start as usize..entry.end as usize])
    }
}

impl Book for RequirementBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_finds_its_own_entries_and_one_with_none_finds_nothing() {
        let mut requirements = RequirementBook::default();
        requirements.push(ActionId::new(1), [[UnitType::new(4)]], []);
        requirements.push(
            ActionId::new(3),
            [
                vec![UnitType::new(2)],
                vec![UnitType::new(5), UnitType::new(6)],
            ],
            [ModifierId::new(0)],
        );
        let of = |at| requirements.of(ActionId::new(at));
        let entries = |at| of(at).map(|required| required.units().collect::<Vec<_>>());
        assert_eq!(entries(1), Some(vec![&[UnitType::new(4)][..]]));
        assert_eq!(
            entries(3),
            Some(vec![
                &[UnitType::new(2)][..],
                &[UnitType::new(5), UnitType::new(6)][..]
            ])
        );
        assert_eq!(of(1).unwrap().modifiers, []);
        assert_eq!(of(3).unwrap().modifiers, [ModifierId::new(0)]);
        assert_eq!([of(0), of(2), of(4)], [None; 3]);
    }
}
