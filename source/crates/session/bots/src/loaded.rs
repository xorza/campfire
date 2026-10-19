use std::sync::Arc;

use campfire_math::Vec3;
use campfire_script::Filter;
use campfire_units::{GroundPoint, OrderRefs};
use campfire_values::{ActionId, ItemId};

/// The references of a bot's order once its start resolved the script's names: a unit by the
/// filter whose nearest unit the order takes as it is due, a slot by its action's name, a location
/// by its point, an item type by its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Loaded;

impl OrderRefs for Loaded {
    type Unit = Filter;
    type Slot = ActionName;
    type Ground = GroundPoint;
    type Point = Vec3;
    type Item = ItemId;
}

/// An action's name as a bot's start resolved it: the actions of that name, one for each package
/// that names one so, which a clone shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionName(pub(crate) Arc<[ActionId]>);
