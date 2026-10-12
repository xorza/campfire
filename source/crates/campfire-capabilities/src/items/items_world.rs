#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{SimWorld, StableId};

    use crate::actions::action_slots::ActionSlots;
    use crate::items::inventory::{Inventory, ItemStack};
    use crate::orders::order::{Action, Order};
    use crate::stats::life::Life;
    use crate::stats::lifetime::Hold;
    use crate::stats::modifiers::Modifiers;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;

    /// What the items module gives a world for its own tests.
    pub(crate) trait ItemsWorldTest {
        /// The carrier of a carrier's match, and the ids its match loads.
        fn carrier(&self) -> Carrier;

        /// Runs a tick in which player 0 orders the carrier each of `actions`.
        fn order_carrier(&mut self, actions: &[Action]);

        /// The carrier's life.
        fn carrier_life(&self) -> Num;

        /// Each slot's item type and count, then its action and the tick it is ready.
        fn carried_items(&self) -> Vec<Option<(usize, u32)>>;

        /// The action in each of the carrier's slots.
        fn carried_actions(&self) -> Vec<Option<ActionId>>;

        /// How many instances of Might the carrier holds, each from itself as a passive.
        fn mights(&self) -> usize;
    }

    impl ItemsWorldTest for World {
        fn carrier(&self) -> Carrier {
            *self.resource::<Carrier>()
        }

        fn order_carrier(&mut self, actions: &[Action]) {
            let unit = self.carrier().id;
            let orders: Vec<Order> = actions
                .iter()
                .map(|&action| Order::one(unit, action))
                .collect();
            let payload = Order::payload(&orders);
            self.push_input(PlayerSlot::new(0), &payload);
            self.step();
        }

        fn carrier_life(&self) -> Num {
            self.part::<Life>(self.carrier().id).current()
        }

        fn carried_items(&self) -> Vec<Option<(usize, u32)>> {
            let inventory = self.part::<Inventory>(self.carrier().id);
            let carried = |slot: &Option<ItemStack>| {
                slot.map(|carried| (carried.item.index(), carried.count.get()))
            };
            inventory.slots().iter().map(carried).collect()
        }

        fn carried_actions(&self) -> Vec<Option<ActionId>> {
            let slots = self.part::<ActionSlots>(self.carrier().id);
            slots.iter().map(|slot| slot.action).collect()
        }

        fn mights(&self) -> usize {
            let carrier = self.carrier();
            let modifiers = self.part::<Modifiers>(carrier.id);
            modifiers
                .iter()
                .filter(|instance| {
                    instance.id == carrier.might
                        && instance.source == Some(carrier.id)
                        && instance.lifetime.held_by(Hold::Passive)
                })
                .count()
        }
    }

    /// A carrier's match: the carrier of three inventory slots, and the drink, flash and Might
    /// its match loads.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Carrier {
        pub(crate) id: StableId,
        pub(crate) drink: ActionId,
        pub(crate) flash: ActionId,
        pub(crate) might: ModifierId,
    }
}
