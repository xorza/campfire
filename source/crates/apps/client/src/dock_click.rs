use bevy::ecs::query::Allow;
use bevy::ecs::system::{Query, Res, SystemParam};
use campfire_actions::{ActionSlots, ActionTarget};
use campfire_capabilities::Order;
use campfire_common::PlayerSlot;
use campfire_declarations::ById;
use campfire_orders::OrdersDirective;
use campfire_production::{GatherSpec, Gatherer, Load, NodeBook, ResourceSet};
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_units::Owner;
use campfire_values::{ActionId, ResourceId, UnitType};

/// What a right click on a unit is to a selection's gatherers (02-engine-core.md, A
/// right-click on a node gathers): a node of its resource, or a drop-off of its player, of the
/// resources it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DockClick {
    Node(ResourceId),
    DropOff {
        owner: PlayerSlot,
        takes: ResourceSet,
    },
}

/// A selected unit as a dock click reads it: its slots, its player and its load.
type Docker = (
    &'static ActionSlots,
    Option<&'static Owner>,
    Option<&'static Gatherer>,
);

/// What a dock click reads of the match: the units by id, the clicked unit's type and player,
/// each selected unit's slots, player and load, and the books of nodes and gathers.
#[derive(SystemParam, Debug)]
pub(crate) struct Docks<'w, 's> {
    index: Res<'w, EntityIndex>,
    clicked: Query<'w, 's, (&'static UnitType, Option<&'static Owner>), Allow<Unpredicted>>,
    selected: Query<'w, 's, Docker, Allow<Unpredicted>>,
    nodes: Res<'w, NodeBook>,
    gathers: Res<'w, ById<ActionId, GatherSpec>>,
}

impl DockClick {
    /// The slot of the gather a unit of `owner`, carrying `load`, casts at the clicked unit, of
    /// its `gathers`, each a slot and the resource it gathers, in its slots' order: at a node, its
    /// first gather of the node's resource; at a drop-off of its own player that takes its load,
    /// its first gather of the load's resource, which returns the load; none else.
    pub(crate) fn slot(
        self,
        owner: Option<PlayerSlot>,
        load: Option<Load>,
        gathers: impl IntoIterator<Item = (u8, ResourceId)>,
    ) -> Option<u8> {
        let wanted = match self {
            Self::Node(resource) => resource,
            Self::DropOff { owner: of, takes } => {
                let load = load.filter(|load| owner == Some(of) && takes.contains(load.resource));
                load?.resource
            }
        };
        let mut gathers = gathers.into_iter();
        gathers.find_map(|(slot, resource)| (resource == wanted).then_some(slot))
    }
}

impl Docks<'_, '_> {
    /// The gather orders of a right click on `target` with `selection` selected, each unit that
    /// gathers or returns there ordered by its own slot; none when the target is neither a node
    /// nor a drop-off of a player, or no selected unit gathers there.
    pub(crate) fn orders(&self, selection: &[StableId], target: StableId) -> Vec<Order> {
        let Some(dock) = self.dock(target) else {
            return Vec::new();
        };
        selection
            .iter()
            .filter_map(|&unit| {
                let (slots, owner, gatherer) = self.selected.get(self.index.get(unit)?).ok()?;
                let held = slots.indexed().filter_map(|(slot, held)| {
                    let spec = self.gathers.get(held.action?)?;
                    Some((slot, spec.resource))
                });
                let load = gatherer.and_then(|gatherer| gatherer.load());
                let slot = dock.slot(owner.copied().map(Owner::player_slot), load, held)?;
                let directive = OrdersDirective::Slot {
                    slot,
                    target: ActionTarget::Unit(target),
                };
                Some(Order::one(unit, directive))
            })
            .collect()
    }

    /// What `target` is to a dock click: a node, or a drop-off of a player; none else.
    fn dock(&self, target: StableId) -> Option<DockClick> {
        let nodes = &self.nodes;
        let (&unit_type, owner) = self.clicked.get(self.index.get(target)?).ok()?;
        if let Some(node) = nodes.nodes.get(unit_type) {
            return Some(DockClick::Node(node.resource));
        }
        let takes = *nodes.drop_offs.get(unit_type)?;
        let owner = owner?.player_slot();
        Some(DockClick::DropOff { owner, takes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dock_click_gathers_a_node_by_its_resource_and_returns_a_load_to_its_own_drop_off() {
        let [gold, wood] = [0, 1].map(ResourceId::new);
        let [own, other] = [0, 1].map(PlayerSlot::new);
        // Slot 1 gathers wood, slot 3 gold: a node of gold takes slot 3, of wood slot 1, whatever
        // the unit carries or whose it is.
        let gathers = [(1, wood), (3, gold)];
        let load = |resource| {
            Some(Load {
                resource,
                amount: 5,
            })
        };
        assert_eq!(DockClick::Node(gold).slot(None, None, gathers), Some(3));
        assert_eq!(
            DockClick::Node(wood).slot(Some(own), load(gold), gathers),
            Some(1)
        );
        let stone = ResourceId::new(2);
        assert_eq!(DockClick::Node(stone).slot(Some(own), None, gathers), None);
        // A drop-off of the unit's player that takes gold returns a load of gold by the gold
        // gather; of another player, with no load, or with a load it does not take, nothing.
        let hall = |owner| DockClick::DropOff {
            owner,
            takes: ResourceSet::from([gold]),
        };
        assert_eq!(hall(own).slot(Some(own), load(gold), gathers), Some(3));
        assert_eq!(hall(other).slot(Some(own), load(gold), gathers), None);
        assert_eq!(hall(own).slot(None, load(gold), gathers), None);
        assert_eq!(hall(own).slot(Some(own), None, gathers), None);
        assert_eq!(hall(own).slot(Some(own), load(wood), gathers), None);
        // A load the unit has no gather of goes back by none.
        assert_eq!(hall(own).slot(Some(own), load(gold), [(1, wood)]), None);
    }
}
