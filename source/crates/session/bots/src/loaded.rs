use std::sync::Arc;

use campfire_math::{Num, Vec3};
use campfire_names::Filter;
use campfire_units::{GroundPoint, OrderRefs};
use campfire_values::{ActionId, ItemId, PerkId};

/// The references of a bot's order once its start resolved the script's names: a unit by the
/// filter whose nearest unit the order takes as it is due, a slot by its action's name, a location
/// by its point, a build's place also by the search the order makes as it is due, an item type by
/// its id, a carried stack by its item type's id, whose first stack the order takes as it is
/// due, and a perk by its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Loaded;

impl OrderRefs for Loaded {
    type Unit = Filter;
    type Slot = ActionName;
    type Ground = GroundPoint;
    type Place = LoadedPlace;
    type Point = Vec3;
    type Item = ItemId;
    type Carried = ItemId;
    type Perk = PerkId;
}

/// A build's place as a bot's start resolved it: the point; or the search round a point that the
/// order makes as it is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadedPlace {
    At(GroundPoint),
    Near {
        near: GroundPoint,
        gap: Num,
        steps: u32,
    },
}

/// An action's name as a bot's start resolved it: the actions of that name, one for each package
/// that names one so, which a clone shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionName(pub(crate) Arc<[ActionId]>);
