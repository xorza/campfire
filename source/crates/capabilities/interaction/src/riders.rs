use std::mem;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_declarations::ById;
use campfire_math::Vec3;
use campfire_sim::{
    DueIndex, EntityIndex, SimComponent, StableId, TimedKept, TimedProcess, TimedRuns,
};
use campfire_states::{Replication, SentOnChange};
use campfire_units::{Dead, UnitEntity};
use campfire_values::{Team, UnitType};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::enter_spec::EnterSpec;
use crate::riding::Riding;

/// A container's riders, in the order they entered, its owner's state
/// (04-capabilities/interaction.md, A container keeps its riders in order): each with the room it
/// takes and whether an exit waits for it; the first tick the next may leave; while a rider's
/// player holds it, the player and the team it had before (04-capabilities/interaction.md, A free
/// building takes its riders' side); the units ordered to enter it, each whether it waits on the
/// ground for it to land; and whether it lands for them (04-capabilities/navigation.md, A flier
/// lands for its riders). Every unit of a container type holds one from its spawn.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
pub struct Riders {
    riders: Vec<Rider>,
    next_exit: Tick,
    held_from: Option<HeldFrom>,
    boarders: Vec<Boarder>,
    lands: bool,
}

/// A unit ordered to enter a container: its stable id, and whether its entry found the container
/// in the air, and it waits for it to land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Boarder {
    pub id: StableId,
    pub waits: bool,
}

/// A rider of a container: its stable id, the room it takes, whether it leaves, and the fire
/// point it holds, of a container of points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rider {
    pub id: StableId,
    pub room: u32,
    pub leaves: bool,
    pub point: Option<u8>,
}

/// The player and the team a container had before its first rider's player took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldFrom {
    pub owner: Option<PlayerSlot>,
    pub team: Team,
}

impl Riders {
    /// Its riders, in the order they entered.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Rider> {
        self.riders.iter()
    }

    pub const fn is_empty(&self) -> bool {
        self.riders.is_empty()
    }

    /// Whether `rider` rides in it.
    pub fn holds(&self, rider: StableId) -> bool {
        self.riders.iter().any(|held| held.id == rider)
    }

    /// The room its riders take.
    pub fn room(&self) -> u32 {
        self.riders.iter().map(|rider| rider.room).sum()
    }

    /// The player and the team it had before a rider's player took it, while one holds it.
    pub const fn held_from(&self) -> Option<HeldFrom> {
        self.held_from
    }

    /// Takes in `rider`, after the others, at the first of `points` fire points no rider holds,
    /// if one is free.
    pub(crate) fn take(&mut self, rider: StableId, room: u32, points: usize) {
        debug_assert!(!self.holds(rider), "a rider rides in one container once");
        let point = (0..points)
            .find(|&at| self.held_by(at).is_none())
            .map(|at| u8::try_from(at).expect("a container's points fit u8"));
        self.riders.push(Rider {
            id: rider,
            room,
            leaves: false,
            point,
        });
    }

    /// The rider that holds the fire point `at`, if one does.
    fn held_by(&self, at: usize) -> Option<StableId> {
        let at = u8::try_from(at).ok()?;
        let held = self.riders.iter().find(|rider| rider.point == Some(at));
        held.map(|rider| rider.id)
    }

    /// Moves `rider` to the free fire point of `points` nearest `target`, when it is nearer than
    /// its own, as Zero Hour's `GarrisonContain::trackTargets` moves a rider to its victim's
    /// nearest point; whether it moved.
    pub(crate) fn aim(&mut self, rider: StableId, points: &[Vec3], target: Vec3) -> bool {
        let distance = |at: usize| points[at].ground_offset(target).length_squared_bits();
        let Some(at) = self.riders.iter().position(|held| held.id == rider) else {
            return false;
        };
        let own = self.riders[at].point.map(usize::from);
        let nearest = (0..points.len())
            .filter(|&point| Some(point) == own || self.held_by(point).is_none())
            .min_by_key(|&point| (distance(point), point));
        if nearest == own {
            return false;
        }
        self.riders[at].point = nearest.map(|point| u8::try_from(point).expect("a point"));
        true
    }

    /// Notes that a rider's player took it from `from`.
    pub(crate) const fn hold_from(&mut self, from: HeldFrom) {
        self.held_from = Some(from);
    }

    /// Gives back what a rider's player took of it, once its last rider left.
    pub(crate) const fn give_back(&mut self) -> Option<HeldFrom> {
        self.held_from.take()
    }

    /// Lets `rider` out after the riders that leave before it; whether it rides in it.
    pub(crate) fn let_out(&mut self, rider: StableId) -> bool {
        let held = self.riders.iter_mut().find(|held| held.id == rider);
        held.map(|held| held.leaves = true).is_some()
    }

    /// Lets every rider out, in their order.
    pub(crate) fn evacuate(&mut self) {
        for rider in &mut self.riders {
            rider.leaves = true;
        }
    }

    /// The first rider that leaves, out of it, when one does and its time came at `now`; the
    /// next leaves `exit` after.
    pub(crate) fn leave(&mut self, now: Tick, exit: Ticks) -> Option<StableId> {
        if now < self.next_exit {
            return None;
        }
        let at = self.riders.iter().position(|rider| rider.leaves)?;
        self.next_exit = now.after(exit);
        Some(self.riders.remove(at).id)
    }

    /// Every rider, out of it, in their order, as it falls.
    pub(crate) fn empty(&mut self) -> Vec<Rider> {
        mem::take(&mut self.riders)
    }

    /// Notes `unit`, ordered to enter it, as one that comes; one that waits stays so.
    pub(crate) fn board(&mut self, unit: StableId) {
        if !self.boarders.iter().any(|boarder| boarder.id == unit) {
            self.boarders.push(Boarder {
                id: unit,
                waits: false,
            });
        }
    }

    /// Notes that `unit`'s entry found it in the air, so it waits for it to land.
    pub(crate) fn wait(&mut self, unit: StableId) {
        match self.boarders.iter_mut().find(|boarder| boarder.id == unit) {
            Some(boarder) => boarder.waits = true,
            None => self.boarders.push(Boarder {
                id: unit,
                waits: true,
            }),
        }
    }

    /// The units that come to it or wait for it, which `keep` keeps or drops.
    pub(crate) const fn boarders_mut(&mut self) -> &mut Vec<Boarder> {
        &mut self.boarders
    }

    /// Whether a unit comes to it, or a rider waits to leave it, for which a flier lands.
    pub(crate) fn wants_ground(&self) -> bool {
        !self.boarders.is_empty() || self.riders.iter().any(|rider| rider.leaves)
    }

    /// Whether it lands for its riders.
    pub(crate) const fn lands(&self) -> bool {
        self.lands
    }

    pub(crate) const fn set_lands(&mut self, lands: bool) {
        self.lands = lands;
    }

    /// The tick its next rider leaves, while one waits to.
    pub(crate) fn exit_due(&self) -> Option<Tick> {
        let leaves = self.riders.iter().any(|rider| rider.leaves);
        leaves.then_some(self.next_exit)
    }
}

/// A living container is due as its next rider may leave, while one waits to; a dead one lets every
/// rider out as it falls (04-capabilities/interaction.md, A rider leaves at a free cell by the
/// container).
impl TimedProcess for Riders {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Has<Dead>);
    type Books = ();

    fn due((): &(), (riders, dead): (&Self, bool)) -> Option<Tick> {
        riders.exit_due().filter(|_| !dead)
    }
}

impl SimComponent for Riders {
    const NAME: &'static str = "interaction.riders";
    type Host = UnitEntity;

    // A container of its type, whose riders each ride in it, once, and fit its room; one a
    // rider's player took holds a rider.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(spec) = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ById<UnitType, EnterSpec>>().get(unit_type))
        else {
            return false;
        };
        let Some(&container) = world.get::<StableId>(entity) else {
            return false;
        };
        let index = world.resource::<EntityIndex>();
        let ride = self.riders.iter().all(|rider| {
            let riding = index.get(rider.id).and_then(|at| world.get::<Riding>(at));
            riding.is_some_and(|riding| riding.container() == container)
        });
        let once = self
            .riders
            .iter()
            .enumerate()
            .all(|(at, rider)| self.riders[..at].iter().all(|held| held.id != rider.id));
        ride && once
            && self.room() <= spec.capacity
            && (self.held_from.is_none() || !self.riders.is_empty())
    }
}

impl Replication for Riders {
    const KIND: DataKind = DataKind::Riders;
    type Sending = SentOnChange;
}
