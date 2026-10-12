#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::orders::ai::Ai;
    use crate::orders::ai_data::AiData;
    use crate::orders::error::AiError;
    use crate::scripts::script_book::ScriptBook;
    use crate::units::by_type::ByType;
    use crate::units::unit_type::UnitType;

    /// What the orders module gives a world for the tests and benches of every crate.
    pub trait OrdersWorldInternals {
        /// Gives `unit_type` its AI, with its compiled script: the think period in milliseconds
        /// becomes whole ticks at the match's rate, rounded up, and at least one.
        fn load_ai(
            &mut self,
            unit_type: UnitType,
            data: &AiData,
            script: ScriptId,
        ) -> Result<(), AiError>;
    }

    impl OrdersWorldInternals for World {
        fn load_ai(
            &mut self,
            unit_type: UnitType,
            data: &AiData,
            script: ScriptId,
        ) -> Result<(), AiError> {
            let rate = *self.resource::<TickRate>();
            let ai = Ai::of(data, script, self.resource::<ScriptBook>(), rate)?;
            self.resource_mut::<ByType<Ai>>().set(unit_type, ai);
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_common::{PlayerSlot, Tick};
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, SimWorld, StableId};

    use crate::actions::action_slots::ActionSlots;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_times::SlotTimes;
    use crate::combat::internals::Armed;
    use crate::items::inventory::{Inventory, ItemStack};
    use crate::navigation::Navigation;
    use crate::navigation::destination::Destination;
    use crate::orders::ai_data::AiData;
    use crate::orders::orders_world::internals::OrdersWorldInternals;
    use crate::players::players_world::PlayersWorld;
    use crate::players::resource_id::ResourceId;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::units::dead::Dead;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::units::units_world::test_world::UnitsWorldTest;
    use crate::values::declared_name::DeclaredName;
    use crate::values::package_path::PackagePath;
    use crate::values::scalar::Scalar;

    /// What the orders module gives a world for its own tests.
    pub(crate) trait OrdersWorldTest {
        /// A hero of `slot` that walks a meter a tick.
        fn hero(&mut self, slot: u32, team: Team, pos: Position, combatant: Armed) -> StableId;

        /// A unit type of `tags` and `params`, and with the AI script `ai` when it has one, thinking
        /// every 250 ms.
        fn ai_type(
            &mut self,
            tags: &[&str],
            params: &[(&str, Scalar)],
            ai: Option<&str>,
        ) -> UnitType;

        /// Runs a tick, and checks that no script call failed in it.
        fn think(&mut self, inputs: &[(u32, &[u8])]);

        fn set_target(&mut self, id: StableId, target: Option<StableId>);

        fn still(&mut self, team: Team, pos: Position, combatant: Armed) -> StableId;

        fn tick(&mut self, inputs: &[(u32, &[u8])]);

        fn position(&self, id: StableId) -> Position;

        fn destination(&self, id: StableId) -> Option<Position>;

        fn slots(&self, id: StableId) -> &ActionSlots;

        fn target(&self, id: StableId) -> Option<StableId>;

        /// When the attack it winds up strikes, if one winds up.
        fn strikes_at(&self, id: StableId) -> Option<Tick>;

        /// When its weapon may attack again.
        fn ready_at(&self, id: StableId) -> Tick;

        fn dead(&self, id: StableId) -> bool;

        /// Player 0's gold.
        fn gold(&self) -> i64;

        /// The items of `id`'s inventory, slot by slot, each by its book's index and its count.
        fn carried(&self, id: StableId) -> Vec<Option<(usize, u32)>>;

        /// The two heroes of a match of two.
        fn heroes(&self) -> [StableId; 2];
    }

    impl OrdersWorldTest for World {
        fn hero(&mut self, slot: u32, team: Team, pos: Position, combatant: Armed) -> StableId {
            let combatant = combatant.bundle(self);
            let parts = (
                combatant,
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
                Owner::new(PlayerSlot::new(slot)),
            );
            self.spawn_unit(pos, team, parts)
        }

        fn ai_type(
            &mut self,
            tags: &[&str],
            params: &[(&str, Scalar)],
            ai: Option<&str>,
        ) -> UnitType {
            let data = UnitTypeData::of(tags, params);
            let unit_type = self.load_next_unit_type(&data);
            if let Some(source) = ai {
                let ai = AiData {
                    ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
                    think_ms: 250,
                };
                let script = self.compile_hooked(source).unwrap();
                self.load_ai(unit_type, &ai, script).unwrap();
            }
            unit_type
        }

        fn think(&mut self, inputs: &[(u32, &[u8])]) {
            self.tick(inputs);
            let failures = self.non_send::<ScriptFailures>().calls();
            assert_eq!(failures, []);
        }

        fn set_target(&mut self, id: StableId, target: Option<StableId>) {
            let mut slots = self.part_mut::<ActionSlots>(id);
            slots.set_attack_target(target);
        }

        fn still(&mut self, team: Team, pos: Position, combatant: Armed) -> StableId {
            let combatant = combatant.bundle(self);
            self.spawn_unit(pos, team, combatant)
        }

        fn tick(&mut self, inputs: &[(u32, &[u8])]) {
            for &(slot, payload) in inputs {
                self.push_input(PlayerSlot::new(slot), payload);
            }
            self.step();
        }

        fn position(&self, id: StableId) -> Position {
            self.try_part::<Position>(id).copied().unwrap()
        }

        fn destination(&self, id: StableId) -> Option<Position> {
            self.try_part::<Destination>(id).copied().unwrap().get()
        }

        fn slots(&self, id: StableId) -> &ActionSlots {
            let entity = self.unit_entity(id);
            self.get::<ActionSlots>(entity).unwrap()
        }

        fn target(&self, id: StableId) -> Option<StableId> {
            self.slots(id).attack_target()
        }

        fn strikes_at(&self, id: StableId) -> Option<Tick> {
            self.slots(id)
                .in_progress()
                .and_then(InProgress::resolves_at)
        }

        fn ready_at(&self, id: StableId) -> Tick {
            self.part::<SlotTimes>(id).slot(0).unwrap().ready_at
        }

        fn dead(&self, id: StableId) -> bool {
            self.try_part::<Dead>(id).is_some()
        }

        fn gold(&self) -> i64 {
            let resources = self.player_resources(PlayerSlot::new(0));
            let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
            resources.unwrap().amount(gold)
        }

        fn carried(&self, id: StableId) -> Vec<Option<(usize, u32)>> {
            let inventory = self.try_part::<Inventory>(id).unwrap();
            let carried = |slot: &Option<ItemStack>| {
                slot.map(|carried| (carried.item.index(), carried.count.get()))
            };
            inventory.slots().iter().map(carried).collect()
        }

        fn heroes(&self) -> [StableId; 2] {
            self.resource::<TwoHeroes>().0
        }
    }

    /// The two heroes of a match of two: slot 0's, then slot 1's.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct TwoHeroes(pub(crate) [StableId; 2]);
}
