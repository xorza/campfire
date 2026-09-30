use bevy_ecs::resource::Resource;

use crate::units::unit_type::UnitType;

/// A value for some of the match's unit types, by type: a capability's package data about each
/// type that has it, such as its AI or its kit.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByType<T: Send + Sync + 'static> {
    entries: Vec<Option<T>>,
}

impl<T: Send + Sync + 'static> ByType<T> {
    pub(crate) fn get(&self, unit_type: UnitType) -> Option<&T> {
        self.entries.get(unit_type.index())?.as_ref()
    }

    /// Gives `unit_type` its `value`, which it has none of yet.
    pub(crate) fn set(&mut self, unit_type: UnitType, value: T) {
        let index = unit_type.index();
        if self.entries.len() <= index {
            self.entries.resize_with(index + 1, || None);
        }
        assert!(self.entries[index].is_none(), "a unit type has one value");
        self.entries[index] = Some(value);
    }
}

impl<T: Send + Sync + 'static> Default for ByType<T> {
    fn default() -> ByType<T> {
        ByType {
            entries: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_has_its_value_or_none() {
        let mut table = ByType::default();
        table.set(UnitType::new(2), 'c');
        table.set(UnitType::new(0), 'a');
        let read = [0, 1, 2, 3].map(|index| table.get(UnitType::new(index)).copied());
        assert_eq!(read, [Some('a'), None, Some('c'), None]);
    }
}
