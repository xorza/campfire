use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, Fact};
use campfire_sim::Capability;
use campfire_values::UnitType;

use crate::action_slots::ActionSlots;
use crate::error::SlotError;
use crate::slot_kind_book::SlotKindBook;
use crate::type_slots_data::TypeSlotsData;

/// The slot kinds whose slots a capability fills of its own, not with a unit type's actions, by
/// the unit types whose sections claim them, as an inventory's are its items' and a use slot its
/// object's: a fact of the slots that the modules of those capabilities add to.
#[derive(Debug, Default)]
pub struct SlotClaims(pub Vec<SlotClaim>);

/// A claim of the slots of `kind` on `unit_type`, of their count, by the capability `by`.
#[derive(Debug, Clone)]
pub struct SlotClaim {
    pub unit_type: UnitType,
    pub kind: DeclaredName,
    pub slots: usize,
    pub by: Capability,
}

impl Fact for SlotClaims {}

impl SlotClaims {
    /// The capability that claims `kind`, if one does.
    pub fn claimant(&self, kind: &DeclaredName) -> Option<Capability> {
        let claim = self.0.iter().find(|claim| claim.kind == *kind);
        claim.map(|claim| claim.by)
    }

    /// The rule of the claimed slot kinds: each is of one rank, as an item's and a use's action
    /// has, and of one capability's claims alone; no unit type of any package puts actions in
    /// one; and no unit type holds more slots, its own and those its claims fill, than a unit
    /// holds. A kind the mode does not declare is the slots' build's problem.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let claims = cx.fact::<Self>()?;
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let fail = |unit_type, problem| {
            let unit = cx.type_of(unit_type);
            BookError::rule(unit.package, BookPlace::UnitType(unit.place()), problem)
        };
        let mut claimed: BTreeMap<&DeclaredName, Capability> = BTreeMap::new();
        for claim in &claims.0 {
            let kind = kinds.names().named(claim.kind.as_str());
            if kind.is_some_and(|kind| kinds.ranks(kind) != 1) {
                let problem = SlotError::RankedClaim(claim.kind.clone());
                return Err(fail(claim.unit_type, problem));
            }
            if *claimed.entry(&claim.kind).or_insert(claim.by) != claim.by {
                let problem = SlotError::ClaimedTwice(claim.kind.clone());
                return Err(fail(claim.unit_type, problem));
            }
        }
        for unit in cx.types() {
            let slots = unit.data.section::<TypeSlotsData>();
            let own = slots.into_iter().flat_map(|slots| &slots.0);
            let slotted = own
                .clone()
                .find_map(|(kind, _)| Some((kind, *claimed.get(kind)?)));
            if let Some((kind, by)) = slotted {
                let kind = kind.clone();
                return Err(fail(unit.unit_type, SlotError::ClaimedSlotted { kind, by }));
            }
            let held: usize = own.map(|(_, ids)| ids.len()).sum();
            let filled = claims
                .0
                .iter()
                .filter(|claim| claim.unit_type == unit.unit_type);
            if held + filled.map(|claim| claim.slots).sum::<usize>() > ActionSlots::LIMIT {
                return Err(fail(unit.unit_type, SlotError::TooMany));
            }
        }
        Ok(())
    }
}
