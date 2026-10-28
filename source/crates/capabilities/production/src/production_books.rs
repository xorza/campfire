use bevy_ecs::system::{Res, SystemParam};
use campfire_actions::ActionBook;
use campfire_common::PlayerSlot;
use campfire_declarations::ById;
use campfire_stats::PlayerModifiers;
use campfire_values::{ActionId, UnitType};

use crate::held_places::HeldPlaces;
use crate::holdings::Holdings;
use crate::producer_spec::ProducerSpec;
use crate::requirement_book::RequirementBook;
use crate::research_spec::ResearchSpec;
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::SupplyCostBook;
use crate::train_queue::TrainQueue;
use crate::train_spec::TrainSpec;

/// The books a train is checked and paid by: the actions, each producer's data, what a train
/// requires and makes, what it costs in supply, and the supply rules, none in a match without
/// supply.
#[derive(SystemParam, Debug)]
pub(crate) struct ProductionBooks<'w> {
    pub actions: Res<'w, ActionBook>,
    pub(crate) producers: Res<'w, ById<UnitType, ProducerSpec>>,
    pub(crate) requirements: Res<'w, RequirementBook>,
    pub(crate) trains: Res<'w, ById<ActionId, TrainSpec>>,
    pub(crate) researches: Res<'w, ById<ActionId, ResearchSpec>>,
    pub(crate) costs: Res<'w, SupplyCostBook>,
    pub(crate) rules: Res<'w, SupplyBook>,
}

/// A train, as what its start checks beyond the core's checks of its slot reads it: its
/// producer's type and queue, the producer's owner and the modifiers the owner holds, and its
/// action.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrainAsk<'a> {
    pub(crate) unit_type: UnitType,
    pub(crate) queue: &'a TrainQueue,
    pub(crate) owner: Option<PlayerSlot>,
    pub(crate) modifiers: Option<&'a PlayerModifiers>,
    pub(crate) action: ActionId,
    pub(crate) places: Option<&'a HeldPlaces>,
}

/// Whether a train passes what its start checks beyond the core's checks of its slot, and the
/// supply it then holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrainAdmission {
    Refused,
    /// It passes, of an owner whose supply does not count.
    Free,
    /// It passes, and holds `supply` of `owner`'s.
    Holds {
        owner: PlayerSlot,
        supply: u32,
    },
}

impl ProductionBooks<'_> {
    /// Whether the train or the research of `ask` passes what its start checks beyond the core's
    /// checks of its slot, by `holdings`: its queue has room, its owner meets its `requires`, a
    /// train at a place has a place its producer's units and queue leave free, and, for a train in
    /// a match with supply, has room under its cap for what it uses; a research uses none.
    pub(crate) fn admits(&self, holdings: &Holdings, ask: TrainAsk<'_>) -> TrainAdmission {
        let TrainAsk {
            unit_type,
            queue,
            owner,
            modifiers,
            action,
            places,
        } = ask;
        let capacity = self
            .producers
            .get(unit_type)
            .map(|production| production.queue);
        let room = capacity.is_some_and(|capacity| queue.has_room(capacity));
        if !room || !holdings.meet(owner, modifiers, self.requirements.of(action)) {
            return TrainAdmission::Refused;
        }
        let Some(made) = self.trains.get(action) else {
            return TrainAdmission::Free;
        };
        if places.is_some_and(|places| !places.admits(action, queue, &self.trains)) {
            return TrainAdmission::Refused;
        }
        let cost = self.costs.cost(made.unit_type);
        let max = self.rules.0.map(|rules| u64::from(rules.max));
        match max.zip(owner) {
            Some((max, owner)) if holdings.has_room(owner, cost, max) => TrainAdmission::Holds {
                owner,
                supply: cost,
            },
            Some(_) => TrainAdmission::Refused,
            None => TrainAdmission::Free,
        }
    }
}
