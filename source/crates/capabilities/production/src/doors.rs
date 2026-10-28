use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::door_times::DoorTimes;
use crate::producer_spec::ProducerSpec;

/// A producer's doors, each by its index: the tick it last opened, the tick the last unit through
/// it left, and whether a unit at its place holds it open, or none for a door that never opened,
/// from which each door's phase follows by its times (defenses-and-airfields.md, A producer's door
/// opens before its unit leaves).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Doors(Box<[Option<OpenedDoor>]>);

/// A door that opened: the tick it opened, or will, once its opening time ends, the tick the last
/// unit through it left, or will, at the earliest as it opened, and whether a unit at its place
/// holds it open, as Zero Hour's `setHoldDoorOpen` holds an airfield's door while its place holds
/// a jet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedDoor {
    pub open_at: Tick,
    pub left: Tick,
    pub held: bool,
}

/// Where a door is in its cycle at a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorPhase {
    Closed,
    Opening,
    Open,
    Closing,
}

impl OpenedDoor {
    /// Its phase at `now`, of `times`: opening until it opened, then open while it is held, in the
    /// tick its last unit leaves, of an open time of none too, and until its open time past it,
    /// then closing for its closing time, then closed.
    pub fn phase(self, now: Tick, times: DoorTimes) -> DoorPhase {
        let closing_at = self.left.after(times.open);
        if now < self.open_at {
            DoorPhase::Opening
        } else if self.held || now == self.left || now < closing_at {
            DoorPhase::Open
        } else if now < closing_at.after(times.closing) {
            DoorPhase::Closing
        } else {
            DoorPhase::Closed
        }
    }

    /// Whether its phase at `now`, of `times`, holds until its door is used again: closed, or held
    /// open.
    pub fn settled(self, now: Tick, times: DoorTimes) -> bool {
        match self.phase(now, times) {
            DoorPhase::Closed => true,
            DoorPhase::Open => self.held,
            DoorPhase::Opening | DoorPhase::Closing => false,
        }
    }

    /// A door that begins to open at `now`, of `times`, held as `held` says.
    const fn opening(now: Tick, times: DoorTimes, held: bool) -> Self {
        let open_at = now.after(times.opening);
        Self {
            open_at,
            left: open_at,
            held,
        }
    }
}

impl Doors {
    /// `count` doors, each closed.
    pub(crate) fn closed(count: u8) -> Self {
        Self(vec![None; usize::from(count)].into())
    }

    /// The door at `door`, none for one that never opened.
    pub fn get(&self, door: u8) -> Option<OpenedDoor> {
        self.0.get(usize::from(door)).copied().flatten()
    }

    /// The index of the door of the place `place`, as Zero Hour gives each of an airfield's places
    /// its door in turn (`ParkingPlaceBehavior::buildInfo`), a door to several places when it has
    /// fewer doors than places.
    fn door_of(&self, place: u8) -> usize {
        usize::from(place) % self.0.len()
    }

    /// The door of the place `place`.
    fn of_place(&mut self, place: u8) -> &mut Option<OpenedDoor> {
        let at = self.door_of(place);
        &mut self.0[at]
    }

    /// The last tick a door opens at, which a unit that waits for it waits until; none while no
    /// door ever opened.
    pub(crate) fn last_open_at(&self) -> Option<Tick> {
        self.0.iter().flatten().map(|door| door.open_at).max()
    }

    /// Lets a unit through the door of the place `place` at `now`, of `times`, as Zero Hour's
    /// `ProductionUpdate` does: through an open or a closing door it leaves now, and the door
    /// stays open its open time from now; a closed door begins to open, and an opening one waits.
    /// None once it left, else the tick it may leave.
    pub(crate) fn pass(&mut self, place: u8, now: Tick, times: DoorTimes) -> Option<Tick> {
        let slot = self.of_place(place);
        match *slot {
            Some(held) if now < held.open_at => Some(held.open_at),
            Some(held) if held.phase(now, times) != DoorPhase::Closed => {
                *slot = Some(OpenedDoor { left: now, ..held });
                None
            }
            _ => {
                let door = OpenedDoor::opening(now, times, false);
                *slot = Some(door);
                Some(door.open_at)
            }
        }
    }

    /// Holds open the door of the place `place` at `now`, of `times`, while a unit holds the place:
    /// a closed door begins to open, as Zero Hour's `setHoldDoorOpen` opens one.
    pub fn hold(&mut self, place: u8, now: Tick, times: DoorTimes) {
        let slot = self.of_place(place);
        *slot = Some(match *slot {
            Some(door) if door.phase(now, times) != DoorPhase::Closed => {
                OpenedDoor { held: true, ..door }
            }
            _ => OpenedDoor::opening(now, times, true),
        });
    }

    /// Lets the door of the place `place` close as its place frees at `now`, unless another of
    /// its places is among `held`, the places that hold a unit: it stays open its open time from
    /// then, or from its opening's end.
    pub(crate) fn release(&mut self, place: u8, now: Tick, held: impl IntoIterator<Item = u8>) {
        let door = self.door_of(place);
        if held.into_iter().any(|other| self.door_of(other) == door) {
            return;
        }
        if let Some(door) = &mut self.0[door]
            && door.held
        {
            door.held = false;
            door.left = now.max(door.open_at);
        }
    }
}

impl SimComponent for Doors {
    const NAME: &'static str = "production.doors";
    type Host = UnitEntity;

    // As many doors as its type's production gives, and none left before it opened.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ById<UnitType, ProducerSpec>>();
        let counted = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| specs.get(unit_type)?.doors)
            .is_some_and(|times| usize::from(times.count) == self.0.len());
        counted
            && self
                .0
                .iter()
                .flatten()
                .all(|door| door.open_at <= door.left)
    }
}

impl Replication for Doors {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::doors::{Doors, OpenedDoor};

    impl Doors {
        /// Doors of `doors`, by place, each that opened or none.
        pub fn of(doors: impl IntoIterator<Item = Option<OpenedDoor>>) -> Self {
            Self(doors.into_iter().collect())
        }
    }
}
