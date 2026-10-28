use campfire_actions::{
    ActionData, ActionKind, ActionsData, SlotClaim, SlotClaims, Targeting, TypeSlotsData,
};
use campfire_declarations::{BookCx, BookError, BookPlace, Section, SourceKind};
use campfire_math::Vec3;
use campfire_sim::Capability;
use campfire_values::{PackageIndex, TypeScope};

use crate::Interaction;
use crate::enter_data::{EnterData, FireData, RiderCount};
use crate::error::{EnterError, UseError};
use crate::rides_data::RidesData;
use crate::use_data::UseData;
use crate::uses_data::UsesData;

impl Interaction {
    /// Claims the use slot of each unit type that holds uses, of its slot kind, as its objects'
    /// uses and its containers' entries fill it.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn claim_slots(cx: &BookCx<'_>, claims: &mut SlotClaims) -> Result<(), BookError> {
        for unit in cx.types() {
            if let Some(uses) = unit.data.section::<UsesData>() {
                claims.0.push(SlotClaim {
                    unit_type: unit.unit_type,
                    kind: uses.kind.clone(),
                    slots: 1,
                    by: Capability::Interaction,
                });
            }
        }
        Ok(())
    }

    /// The rule of the uses and the entries: each `use` section names an action of its package of
    /// the `use` kind, and each `enter` section one of the `enter` kind, of a capacity, whose
    /// payload rides in it; a type that rides uses; no unit type's slots and no loadout name a
    /// use or an entry, as each runs only from its object; and each aims at a unit its filter
    /// selects. A name the package does not have is the books' problem.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let kind = |package, id| {
            let actions = cx.package(package).content.get::<ActionsData>();
            let action = actions.and_then(|actions| actions.0.get(id));
            action.map(ActionData::kind)
        };
        for unit in cx.types() {
            let at = || BookPlace::UnitType(unit.place());
            if let Some(usable) = unit.data.section::<UseData>() {
                let used = kind(unit.package, &usable.action);
                if used.is_some_and(|used| used != ActionKind::Use) {
                    let problem = UseError::NotUse(usable.action.clone());
                    return Err(BookError::rule(unit.package, at(), problem));
                }
            }
            if let Some(enter) = unit.data.section::<EnterData>() {
                let entered = kind(unit.package, &enter.action);
                if entered.is_some_and(|entered| entered != ActionKind::Enter) {
                    let problem = EnterError::NotEnter(enter.action.clone());
                    return Err(BookError::rule(unit.package, at(), problem));
                }
                Self::enter_rule(cx, unit.package, enter)
                    .map_err(|problem| BookError::rule(unit.package, at(), problem))?;
            }
            if unit.data.section::<RidesData>().is_some()
                && unit.data.section::<UsesData>().is_none()
            {
                let problem = EnterError::RidesWithoutUses;
                return Err(BookError::rule(unit.package, at(), problem));
            }
            let slots = unit.data.section::<TypeSlotsData>().into_iter();
            for id in slots.flat_map(|slots| slots.0.values()).flatten() {
                match kind(unit.package, id) {
                    Some(ActionKind::Use) => {
                        let problem = UseError::Named(id.clone());
                        return Err(BookError::rule(unit.package, at(), problem));
                    }
                    Some(ActionKind::Enter) => {
                        let problem = EnterError::Named(id.clone());
                        return Err(BookError::rule(unit.package, at(), problem));
                    }
                    _ => {}
                }
            }
        }
        // An entry and a use aim at a unit, and no loadout declares one, as a loadout's actions
        // are a hero's.
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            let objects = actions
                .flat_map(|actions| &actions.0)
                .filter(|(_, action)| matches!(action.kind(), ActionKind::Use | ActionKind::Enter));
            for (id, action) in objects {
                let aims = matches!(action.keys.targeting, Targeting::Unit(_));
                let loadout = matches!(source.kind, SourceKind::Loadout);
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                let error = match (action.kind(), aims, loadout) {
                    (_, true, false) => continue,
                    (ActionKind::Use, false, _) => BookError::rule(package, at, UseError::Aims),
                    (ActionKind::Use, true, true) => {
                        BookError::rule(package, at, UseError::Named(id.clone()))
                    }
                    (_, false, _) => BookError::rule(package, at, EnterError::Aims),
                    (_, true, true) => BookError::rule(package, at, EnterError::Named(id.clone())),
                };
                return Err(error);
            }
        }
        Ok(())
    }

    /// The rule of an `enter` section of `package`: a capacity, fire points within a place's
    /// bound and no more than a rider's point holds, and a payload of types that ride, which its
    /// room holds.
    fn enter_rule(
        cx: &BookCx<'_>,
        package: PackageIndex,
        enter: &EnterData,
    ) -> Result<(), EnterError> {
        if enter.capacity == 0 {
            return Err(EnterError::NoCapacity);
        }
        let bound = Vec3::BOUND.to_bits().unsigned_abs();
        if let FireData::Points(points) = &enter.fire
            && (points.len() > usize::from(u8::MAX) + 1
                || points
                    .iter()
                    .flatten()
                    .any(|at| at.to_bits().unsigned_abs() > bound))
        {
            return Err(EnterError::FirePoint);
        }
        let scope = TypeScope::of_package(package);
        let mut room = 0_u64;
        for payload in &enter.payload {
            let Some(unit_type) = cx.unit_type(scope, payload.unit.as_str()) else {
                continue;
            };
            let rides = cx.type_of(unit_type).data.section::<RidesData>();
            let rides = rides.ok_or(EnterError::Payload)?;
            let each = match enter.counts {
                RiderCount::Size => rides.size.get(),
                RiderCount::One => 1,
            };
            room += u64::from(each) * u64::from(payload.count);
        }
        if room > u64::from(enter.capacity) {
            return Err(EnterError::Payload);
        }
        Ok(())
    }
}
