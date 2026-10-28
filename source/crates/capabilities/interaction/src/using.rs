use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_actions::{ActionSlots, SlotKind};
use campfire_declarations::ById;
use campfire_players::OnMatch;
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_units::Dead;
use campfire_values::{ActionId, Relations, TagSet, Team, UnitTags, UnitType};

use crate::enter_spec::EnterSpec;
use crate::rider_size::RiderSize;
use crate::use_slot::UseSlot;
use crate::use_spec::UseSpec;

/// The uses as an order checks them: whether a unit may use an object, and with which slot and
/// action. A client reads the objects it holds, the server's among them, and the users it
/// predicts.
#[derive(SystemParam, Debug)]
pub struct Using<'w, 's> {
    index: Res<'w, EntityIndex>,
    specs: Res<'w, ById<UnitType, UseSpec>>,
    entries: Res<'w, ById<UnitType, EnterSpec>>,
    sizes: Res<'w, ById<UnitType, RiderSize>>,
    slots: Res<'w, ById<UnitType, UseSlot>>,
    relations: OnMatch<'w, 's, Relations>,
    objects: Objects<'w, 's>,
    users: Users<'w, 's>,
}

/// A living unit's type and team, whether a client predicts it or not.
type Objects<'w, 's> =
    Query<'w, 's, (&'static UnitType, &'static Team), (Without<Dead>, Allow<Unpredicted>)>;

/// A living unit's type, team, tags and slots.
type Users<'w, 's> = Query<
    'w,
    's,
    (
        &'static UnitType,
        &'static Team,
        Option<&'static UnitTags>,
        &'static ActionSlots,
    ),
    Without<Dead>,
>;

/// A use a unit may start: its use slot, and the object's action that slot runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitUse {
    pub slot: u8,
    pub action: ActionId,
}

impl Using<'_, '_> {
    /// The use of the living unit `object` by the living unit of `user`: none when the object's
    /// type has no `use`, the user's no `uses`, or the object's users do not select the user, by
    /// how the object's team regards the user's and the user's tags.
    pub fn of(&self, user: Entity, object: StableId) -> Option<UnitUse> {
        let (specs, slots) = (&self.specs, &self.slots);
        let (&object_type, &object_team) = self.objects.get(self.index.get(object)?).ok()?;
        let spec = *specs.get(object_type)?;
        let (&user_type, &team, tags, held) = self.users.get(user).ok()?;
        let kind = slots.get(user_type)?.kind();
        let relation = self.relations.get().between(object_team, team);
        let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
        if !spec.admits(relation, tags) {
            return None;
        }
        Some(UnitUse {
            slot: Self::use_slot(held, kind),
            action: spec.action(),
        })
    }

    /// The entry of the living unit `container` by the living unit of `rider`: none when the
    /// container's type has no `enter`, the rider's no `uses` or no size, or the container's
    /// riders' filter does not select the rider. Whether the container takes it, its room and its
    /// side among them, its resolve tests (transports.md, An entry is an action of the
    /// container's).
    pub fn entry(&self, rider: Entity, container: StableId) -> Option<UnitUse> {
        let (&container_type, &container_team) =
            self.objects.get(self.index.get(container)?).ok()?;
        let spec = self.entries.get(container_type)?;
        let (&rider_type, &team, tags, held) = self.users.get(rider).ok()?;
        self.sizes.get(rider_type)?;
        let kind = self.slots.get(rider_type)?.kind();
        let relation = self.relations.get().between(container_team, team);
        let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
        spec.admits(relation, tags).then(|| UnitUse {
            slot: Self::use_slot(held, kind),
            action: spec.action,
        })
    }

    /// The slot of the use slot kind `kind` of a unit of `held` slots.
    fn use_slot(held: &ActionSlots, kind: SlotKind) -> u8 {
        let slot = held.first_of(kind);
        debug_assert!(
            held.slot(slot).is_some_and(|at| at.kind == kind),
            "a user spawns with its use slot"
        );
        slot
    }
}
