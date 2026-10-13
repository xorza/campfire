use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::declare::per_type::PerType;

use crate::scripts::frame::Frame;
use crate::units::unit_type::UnitType;
use crate::units::view::View;

/// A value for some of the match's unit types, by type: a capability's package data about each
/// type that has it, such as its AI or a part its units spawn with.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByType<T: Send + Sync + 'static> {
    entries: Vec<Option<T>>,
}

impl<T: Send + Sync + 'static> ByType<T> {
    pub(crate) fn get(&self, unit_type: UnitType) -> Option<&T> {
        self.entries.get(unit_type.index())?.as_ref()
    }

    /// The values of the types that have one, by type.
    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().flatten()
    }

    /// Gives `unit_type` `value`, when it is some, as `set` does.
    pub(crate) fn set_some(&mut self, unit_type: UnitType, value: Option<T>) {
        if let Some(value) = value {
            self.set(unit_type, value);
        }
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

impl<T: PerType> Book for ByType<T> {
    const OWNER: Option<Capability> = T::OWNER;
    const SCRIPTS_ONLY: bool = T::SCRIPTS_ONLY;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_has_its_value_or_none() {
        let mut table = ByType::default();
        table.set(UnitType::new(2), 'c');
        table.set(UnitType::new(0), 'a');
        // None gives a type no value, within the table or past its end.
        table.set_some(UnitType::new(1), None);
        table.set_some(UnitType::new(4), None);
        let read = [0, 1, 2, 3, 4].map(|index| table.get(UnitType::new(index)).copied());
        assert_eq!(read, [Some('a'), None, Some('c'), None, None]);
        assert_eq!(table.values().collect::<Vec<_>>(), [&'a', &'c']);
        table.set_some(UnitType::new(1), Some('b'));
        assert_eq!(table.values().collect::<Vec<_>>(), [&'a', &'b', &'c']);
    }
}
