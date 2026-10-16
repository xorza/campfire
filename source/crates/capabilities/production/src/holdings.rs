use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_common::PlayerSlot;
use campfire_declare::Scratch;
use campfire_stats::PlayerModifiers;
use campfire_units::Owner;
use campfire_values::UnitType;

use crate::requirement_book::Required;
use crate::supply::SupplyPartsItem;

/// What the players hold as a tick's trains and builds start: their living, complete units, by
/// type, each once, in order. The first train or build of the Start set that needs them reads
/// them, and the others of the set find them, as nothing between them changes what the players
/// hold; the set's end forgets them. A build's check, in its own set, reads and forgets its own.
#[derive(Resource, Debug, Default)]
pub(crate) struct Holdings {
    units: Vec<Held>,
    read: bool,
}

/// A living, complete unit a player owns, by its type, for a requirement to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Held {
    owner: PlayerSlot,
    unit_type: UnitType,
}

impl Holdings {
    /// Reads the holdings from `units`, the units the players own, unless they are read.
    pub(crate) fn read<'w, 's>(
        &mut self,
        units: impl IntoIterator<Item = (&'w Owner, SupplyPartsItem<'w, 's>)>,
    ) {
        if self.read {
            return;
        }
        self.units.clear();
        self.units
            .extend(units.into_iter().filter_map(|(owner, parts)| {
                parts.held().map(|unit_type| Held {
                    owner: owner.slot(),
                    unit_type,
                })
            }));
        self.units.sort_unstable();
        self.units.dedup();
        self.read = true;
    }

    /// Forgets the holdings, so the next read reads them anew.
    pub(crate) fn forget(&mut self) {
        self.units.clear();
        self.read = false;
    }

    /// Forgets the holdings as the Start set ends.
    pub(crate) fn forget_read(mut holdings: ResMut<'_, Self>) {
        holdings.forget();
    }

    /// Whether `owner`, who holds `modifiers`, holds what `required` needs: one with no
    /// `requires` needs nothing, and a unit no player owns meets no need.
    pub(crate) fn meet(
        &self,
        owner: Option<PlayerSlot>,
        modifiers: Option<&PlayerModifiers>,
        required: Option<Required<'_>>,
    ) -> bool {
        debug_assert!(
            self.read,
            "the holdings are read before a requirement reads them"
        );
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
        let modifiers = required
            .modifiers
            .iter()
            .all(|&modifier| modifiers.is_some_and(|held| held.holds(modifier)));
        units && modifiers
    }
}

impl Scratch for Holdings {
    fn is_clear(&self) -> bool {
        !self.read && self.units.is_empty()
    }
}
