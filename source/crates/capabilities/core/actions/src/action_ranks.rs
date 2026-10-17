use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, Fact, Section, SourceKind, UnitTypeData};
use campfire_values::PackageIndex;

use crate::actions_data::ActionsData;
use crate::error::ActionProblem;
use crate::loadout_ranks::LoadoutRanks;
use crate::slot_kinds::SlotKinds;
use crate::type_slots_data::TypeSlotsData;

/// The ranks of each package's actions, as their slots give them, package by package: what an
/// action loads with, and the ranks a way of a modifier applies it at.
#[derive(Debug, Default)]
pub(crate) struct ActionRanks(Vec<PackageRanks>);

/// The ranks of one package's actions.
#[derive(Debug)]
enum PackageRanks {
    /// The mode's and an avatar's: of the kinds their types that stand place each action in, 1
    /// for one none places.
    Slotted(BTreeMap<DeclaredName, u8>),
    /// A loadout's: the loadout's, for every action.
    Loadout(u8),
    /// A rules package's, whose actions no match loads.
    Unloaded,
}

impl Fact for ActionRanks {}

impl ActionRanks {
    /// Every package's ranks; an error for an action placed in slot kinds of different ranks,
    /// or a loadout's placed by a summon in kinds of other than the loadout's ranks, as the
    /// action loads with the loadout's.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let kinds = cx.mode_section::<SlotKinds>();
        let none = SlotKinds::default();
        let kinds = kinds.unwrap_or(&none);
        let loadout = cx.fact_if::<LoadoutRanks>()?.map_or(1, |ranks| ranks.0);
        let mut ranks = Vec::with_capacity(cx.source().packages.len());
        for (index, package) in cx.packages() {
            let standing = package.units.values().filter(|unit| !unit.delivers());
            ranks.push(match package.kind {
                SourceKind::Mode => PackageRanks::Slotted(Self::slotted(index, kinds, standing)?),
                SourceKind::Avatar(unit) => {
                    PackageRanks::Slotted(Self::slotted(index, kinds, standing.chain([unit]))?)
                }
                SourceKind::Loadout => {
                    let placed = Self::slotted(index, kinds, standing)?;
                    if let Some((id, _)) = placed.iter().find(|&(_, &ranks)| ranks != loadout) {
                        return Err(Self::error(index, id));
                    }
                    PackageRanks::Loadout(loadout)
                }
                SourceKind::Rules => PackageRanks::Unloaded,
            });
        }
        Ok(Self(ranks))
    }

    /// The rule of the actions' ranks: as `build` gives them.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        cx.fact::<Self>().map(drop)
    }

    /// The ranks of the action `id` of the package at `package`; none for a rules package's.
    pub(crate) fn of(&self, package: PackageIndex, id: &str) -> Option<u8> {
        match &self.0[usize::from(package)] {
            PackageRanks::Slotted(ranks) => Some(ranks.get(id).copied().unwrap_or(1)),
            &PackageRanks::Loadout(ranks) => Some(ranks),
            PackageRanks::Unloaded => None,
        }
    }

    /// The ranks the types of `slots`, of the package at `index`, give each action they place;
    /// an error for an action placed in slot kinds of different ranks.
    fn slotted<'u>(
        index: PackageIndex,
        kinds: &SlotKinds,
        types: impl IntoIterator<Item = &'u UnitTypeData>,
    ) -> Result<BTreeMap<DeclaredName, u8>, BookError> {
        let slots = types
            .into_iter()
            .filter_map(|unit| unit.section::<TypeSlotsData>());
        let ranks = kinds.slotted_ranks(slots.map(|slots| &slots.0));
        let ranks = ranks.map_err(|action| Self::error(index, action))?;
        let owned = ranks.into_iter().map(|(id, ranks)| {
            let id = DeclaredName::new(id).expect("an action's id is a declared name");
            (id, ranks)
        });
        Ok(owned.collect())
    }

    fn error(package: PackageIndex, action: &DeclaredName) -> BookError {
        let at = BookPlace::Entry {
            table: ActionsData::KEY,
            name: action.clone(),
        };
        BookError::rule(package, at, ActionProblem::SlottedRanks)
    }
}
