use std::fmt::Debug;

use bevy_ecs::world::{EntityRef, EntityWorldMut};
use campfire_math::Num;
use campfire_sim::{Position, SimComponent, StateRegistry};

use crate::combat::recent_attackers::RecentAttackers;
use crate::items::inventory::Inventory;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::destination::Destination;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::production::builder::Builder;
use crate::production::gatherer::Gatherer;
use crate::production::node::Node;
use crate::production::train_queue::TrainQueue;
use crate::progression::experience::Experience;
use crate::progression::points::Points;
use crate::stats::level::Level;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::Modifiers;
use crate::units::facing::Facing;
use crate::units::spawn_point::SpawnPoint;
use crate::units::track_id::TrackId;

/// A state type a unit of the mode spawns with, when its kit gives it. `ModeSpec::spawn` gives
/// each part a unit's kit gives, and a unit loses none while it stands, so a restore checks that
/// each unit holds each part its kit gives, of the types the match's state holds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KitPart(&'static dyn Part);

impl KitPart {
    /// Every part, in the order a spawn gives them.
    const ALL: [KitPart; 22] = [
        KitPart(&Given(|at| Some(Facing::of(at.angle)))),
        KitPart(&Given(|at| Some(SpawnPoint::new(at.pos)))),
        KitPart(&Given(|_| Some(Level::default()))),
        KitPart(&Given(|_| Some(Modifiers::default()))),
        KitPart(&Given(|_| Some(ModifierClocks::default()))),
        KitPart(&Given(|at| at.kit.life)),
        KitPart(&Given(|at| at.kit.pools)),
        KitPart(&Given(|at| at.kit.on_death)),
        KitPart(&Given(|at| {
            at.kit.on_death.map(|_| RecentAttackers::default())
        })),
        KitPart(&Given(|at| at.kit.sight)),
        KitPart(&Given(|at| at.kit.body.map(|form| form.at(at.angle)))),
        KitPart(&Given(|at| at.kit.step)),
        KitPart(&Given(|at| at.kit.step.map(|_| Destination::default()))),
        KitPart(&Given(|at| at.kit.step.map(|_| Route::default()))),
        KitPart(&Given(|at| at.kit.step.map(|_| Progress::default()))),
        KitPart(&Given(|at| {
            let tracks = at.kit.tracks;
            (!tracks.is_empty()).then(|| Experience::new(tracks, at.level_track))
        })),
        KitPart(&Given(|at| Points::at_spawn(at.kit.tracks, at.level_track))),
        KitPart(&Given(|at| at.kit.queue.map(|_| TrainQueue::default()))),
        KitPart(&Given(|at| at.kit.builds.then(Builder::default))),
        KitPart(&Given(|at| {
            at.kit.node.map(|amount| Node::new(amount.get()))
        })),
        KitPart(&Given(|at| at.kit.gathers.then(Gatherer::default))),
        KitPart(&Given(|at| {
            let inventory = at.kit.inventory?;
            Some(Inventory::new(inventory.slots, inventory.kind))
        })),
    ];

    /// Gives `unit`, spawning `at`, every part its kit gives.
    pub(crate) fn give_every(at: &KitSpawn<'_>, unit: &mut EntityWorldMut<'_>) {
        for part in KitPart::ALL {
            part.0.give(at, unit);
        }
    }

    /// The parts whose types the state of a match of `registry` holds: a part of a capability
    /// the mode does not declare is no state, and a restore never brings it back.
    pub(crate) fn of_state(registry: &StateRegistry) -> Vec<KitPart> {
        KitPart::ALL
            .into_iter()
            .filter(|part| part.0.registered(registry))
            .collect()
    }

    /// Whether `unit`, spawned `at`, holds the part, or its kit gives none.
    pub(crate) fn held_by(self, unit: EntityRef<'_>, at: &KitSpawn<'_>) -> bool {
        !self.0.given(at) || self.0.held(unit)
    }
}

/// What a unit's parts are made of as it spawns: its kit, the match's level track, its place
/// and its angle. Whether a kit gives a part is the kit's and the track's alone; the place and
/// the angle shape only the part's value.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KitSpawn<'a> {
    pub(crate) kit: &'a UnitKit,
    pub(crate) level_track: Option<TrackId>,
    pub(crate) pos: Position,
    pub(crate) angle: Num,
}

/// One part, of whatever type, as `KitPart` holds it.
trait Part: Debug + Sync {
    fn registered(&self, registry: &StateRegistry) -> bool;
    fn held(&self, unit: EntityRef<'_>) -> bool;
    fn given(&self, at: &KitSpawn<'_>) -> bool;
    fn give(&self, at: &KitSpawn<'_>, unit: &mut EntityWorldMut<'_>);
}

/// A part of type `C`, made from a spawn's inputs; `None` when the kit gives none.
#[derive(Debug)]
struct Given<C>(fn(&KitSpawn<'_>) -> Option<C>);

impl<C: SimComponent + Debug> Part for Given<C> {
    fn registered(&self, registry: &StateRegistry) -> bool {
        registry.holds::<C>()
    }

    fn held(&self, unit: EntityRef<'_>) -> bool {
        unit.contains::<C>()
    }

    fn given(&self, at: &KitSpawn<'_>) -> bool {
        (self.0)(at).is_some()
    }

    fn give(&self, at: &KitSpawn<'_>, unit: &mut EntityWorldMut<'_>) {
        if let Some(part) = (self.0)(at) {
            unit.insert(part);
        }
    }
}
