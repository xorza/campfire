#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::{Num, Vec3};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{IdAllocator, Position, SimWorld, StableId};

    use crate::actions::action_book::ActionBook;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::action_target::ActionTarget;
    use crate::actions::slot_kind::SlotKind;
    use crate::navigation::Navigation;
    use crate::orders::order::{Action, Order};
    use crate::players::players_world::PlayersWorld;
    use crate::players::resource_id::ResourceId;
    use crate::production::build_target::BuildTarget;
    use crate::production::builder::Builder;
    use crate::production::gatherer::{GatherStep, Gatherer};
    use crate::production::node::Node;
    use crate::production::site::Site;
    use crate::production::supply_costs::SupplyCosts;
    use crate::production::supply_data::SupplyData;
    use crate::production::supply_rules::SupplyRules;
    use crate::production::train_queue::TrainQueue;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::stats::life::Life;
    use crate::units::action_id::ActionId;
    use crate::units::body::Body;
    use crate::units::body_form::BodyForm;
    use crate::units::by_type::ByType;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::spawn_at::SpawnAt;
    use crate::units::team::Team;
    use crate::units::team_set::TeamSet;
    use crate::units::unit_core::UnitCore;
    use crate::units::unit_type::UnitType;
    use crate::values::rank::Rank;
    use crate::vision::seen_by::SeenBy;

    /// What the production module gives a world for its own tests.
    pub(crate) trait ProductionWorldTest {
        /// The unit types and the gold of a shop's match.
        fn shop(&self) -> Shop;

        /// Each spawn production asked for so far, in order.
        fn spawned(&self) -> &[Spawned];

        /// Counts supply, up to `max`, with each type's `supplies`, and the trains the book holds.
        fn count_supply(&mut self, max: u32, supplies: &[(UnitType, SupplyData)]);

        /// A unit of `unit_type` owned by `owner`, at the origin.
        fn owned(&mut self, unit_type: UnitType, owner: PlayerSlot) -> StableId;

        /// The trains in `producer`'s queue.
        fn queued(&self, producer: StableId) -> usize;

        /// A producer `id` of `team` at `x` meters, owned by `owner`, with `train` in its slot 0
        /// and `queue`.
        fn producer(
            &mut self,
            id: StableId,
            team: u8,
            x: i64,
            owner: Option<PlayerSlot>,
            train: ActionId,
            queue: TrainQueue,
        );

        /// The next stable id.
        fn allocate_id(&mut self) -> StableId;

        /// Orders `producer` to train what its slot 0 holds.
        fn order_train(&mut self, producer: StableId);

        /// Each spawn so far, by producer: the team and position of the producer it came from,
        /// and its owner.
        fn spawns(&self) -> Vec<(Team, Position, Option<PlayerSlot>)>;

        /// Runs a tick in which player `slot` orders `action` to `id`.
        fn tick_ordering(&mut self, slot: u32, id: StableId, action: Action);

        /// Runs a tick in which player `slot` gives `orders`, each an action to a unit, in order.
        fn give_orders(&mut self, slot: u32, orders: &[(StableId, Action)]);

        /// Player 0's gold.
        fn gold(&self) -> i64;

        /// The build of a yard's match.
        fn yard(&self) -> Yard;

        /// A builder of `team`, owned by player `owner`, at `pos`, of a half meter, that walks a
        /// meter a tick, with the build in its slot 0.
        fn builder(&mut self, pos: Position, team: u8, owner: u32) -> StableId;

        /// A walker of `team` at `pos`, of a half meter, seen by `seen`, by all when `None`.
        fn yard_walker(&mut self, pos: Position, team: u8, seen: Option<TeamSet>) -> StableId;

        /// Runs a tick in which player 0 orders `builder` to build its depot at (`x`, `z`).
        fn build_at(&mut self, builder: StableId, x: i64, z: i64);

        /// The depots spawned so far, by stable id.
        fn depots(&self) -> Vec<StableId>;

        /// The depot `id`'s life, and its progress while a site.
        fn site(&self, id: StableId) -> (Num, Option<Num>);

        /// Where `builder`'s build order builds.
        fn order_of(&self, builder: StableId) -> Option<BuildTarget>;

        /// The gather and the mineral type of a mine's match.
        fn mine(&self) -> Mine;

        /// A worker of player `owner` at `pos`, of a half meter, walking a meter a tick, with the
        /// gather in its slot 0.
        fn worker(&mut self, pos: Position, owner: u32) -> StableId;

        /// A worker as `worker` gives, its gather at `rank`, or not learned with none.
        fn worker_ranked(&mut self, pos: Position, owner: u32, rank: Option<Rank>) -> StableId;

        /// A mineral at `pos` holding `amount`.
        fn mineral(&mut self, pos: Position, amount: u32) -> StableId;

        /// Player 0's hall at `pos`.
        fn hall(&mut self, pos: Position) -> StableId;

        /// Runs a tick in which player 0 orders each of `workers` to gather at `node`.
        fn gather_at(&mut self, workers: &[StableId], node: StableId);

        /// What `node` holds, none once gone.
        fn node_amount(&self, node: StableId) -> Option<u32>;

        /// The step of `worker`'s gather loop, none when it gathers nothing.
        fn gather_step(&self, worker: StableId) -> Option<GatherStep>;

        /// The node `worker`'s loop is at.
        fn node_of(&self, worker: StableId) -> Option<StableId>;

        /// The worker that holds `node`.
        fn holder(&self, node: StableId) -> Option<StableId>;
    }

    impl ProductionWorldTest for World {
        fn shop(&self) -> Shop {
            *self.resource::<Shop>()
        }

        fn spawned(&self) -> &[Spawned] {
            &self.resource::<SpawnLog>().0
        }

        fn count_supply(&mut self, max: u32, supplies: &[(UnitType, SupplyData)]) {
            let mut types = ByType::default();
            for &(unit_type, supply) in supplies {
                types.set(unit_type, supply);
            }
            let costs = SupplyCosts::new(types, self.resource::<ActionBook>());
            self.insert_resource(costs);
            self.insert_resource(SupplyRules { max });
        }

        fn owned(&mut self, unit_type: UnitType, owner: PlayerSlot) -> StableId {
            self.spawn_unit_of(unit_type, Position::ORIGIN, Team::new(1), Owner::new(owner))
        }

        fn queued(&self, producer: StableId) -> usize {
            self.part::<TrainQueue>(producer).entries().len()
        }

        fn producer(
            &mut self,
            id: StableId,
            team: u8,
            x: i64,
            owner: Option<PlayerSlot>,
            train: ActionId,
            queue: TrainQueue,
        ) {
            let slots = ActionSlots::new([(train, SlotKind::new(0), Rank::new(1))]).timed();
            let core = UnitCore {
                id,
                pos: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap(),
                unit_type: self.shop().barracks,
                team: Team::new(team),
                owner: owner.map(Owner::new),
            };
            self.spawn_core(core, (slots, queue));
        }

        fn allocate_id(&mut self) -> StableId {
            self.resource_mut::<IdAllocator>().allocate()
        }

        fn order_train(&mut self, producer: StableId) {
            let mut slots = self.part_mut::<ActionSlots>(producer);
            slots.order(0, ActionTarget::None);
        }

        fn spawns(&self) -> Vec<(Team, Position, Option<PlayerSlot>)> {
            let grunt = self.shop().grunt;
            let spawned = self.spawned();
            assert!(spawned.iter().all(|spawn| spawn.at.unit_type == grunt));
            spawned
                .iter()
                .map(|spawn| (spawn.at.team, spawn.at.pos, spawn.owner))
                .collect()
        }

        fn tick_ordering(&mut self, slot: u32, id: StableId, action: Action) {
            self.give_orders(slot, &[(id, action)]);
        }

        fn give_orders(&mut self, slot: u32, orders: &[(StableId, Action)]) {
            let orders: Vec<Order> = orders
                .iter()
                .map(|&(unit, action)| Order::one(unit, action))
                .collect();
            let payload = Order::payload(&orders);
            self.push_input(PlayerSlot::new(slot), &payload);
            self.step();
        }

        fn gold(&self) -> i64 {
            self.player_resources(PlayerSlot::new(0))
                .unwrap()
                .amount(self.shop().gold)
        }

        fn yard(&self) -> Yard {
            *self.resource::<Yard>()
        }

        fn builder(&mut self, pos: Position, team: u8, owner: u32) -> StableId {
            let build = self.yard().build;
            let slots = ActionSlots::new([(build, SlotKind::new(0), Rank::new(1))]).timed();
            let parts = (
                Owner::new(PlayerSlot::new(owner)),
                Body::new(Num::HALF).unwrap(),
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
                slots,
                Builder::default(),
            );
            self.spawn_unit(pos, Team::new(team), parts)
        }

        fn yard_walker(&mut self, pos: Position, team: u8, seen: Option<TeamSet>) -> StableId {
            let parts = (
                Body::new(Num::HALF).unwrap(),
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            );
            let id = self.spawn_unit(pos, Team::new(team), parts);
            if let Some(teams) = seen {
                self.insert_parts(id, SeenBy::new(teams));
            }
            id
        }

        fn build_at(&mut self, builder: StableId, x: i64, z: i64) {
            let target = BuildTarget::Point {
                x: Num::int(x),
                z: Num::int(z),
                angle: Num::ZERO,
            };
            self.tick_ordering(0, builder, Action::Build { slot: 0, target });
        }

        fn depots(&self) -> Vec<StableId> {
            let depot = self.shop().depot;
            self.spawned()
                .iter()
                .filter(|spawn| spawn.at.unit_type == depot)
                .map(|spawn| spawn.at.id)
                .collect()
        }

        fn site(&self, id: StableId) -> (Num, Option<Num>) {
            let life = self.part::<Life>(id).current();
            let progress = self.try_part::<Site>(id).map(Site::progress);
            (life, progress)
        }

        fn order_of(&self, builder: StableId) -> Option<BuildTarget> {
            self.part::<Builder>(builder)
                .order()
                .map(|order| order.target)
        }

        fn mine(&self) -> Mine {
            *self.resource::<Mine>()
        }

        fn worker(&mut self, pos: Position, owner: u32) -> StableId {
            self.worker_ranked(pos, owner, Rank::new(1))
        }

        fn worker_ranked(&mut self, pos: Position, owner: u32, rank: Option<Rank>) -> StableId {
            let gather = self.mine().gather;
            let slots = ActionSlots::new([(gather, SlotKind::new(0), rank)]).timed();
            let parts = (
                Owner::new(PlayerSlot::new(owner)),
                Body::new(Num::HALF).unwrap(),
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
                slots,
                Gatherer::default(),
            );
            self.spawn_unit(pos, Team::new(0), parts)
        }

        fn mineral(&mut self, pos: Position, amount: u32) -> StableId {
            let body = BodyForm::box_sized([Num::int(2), Num::ONE])
                .unwrap()
                .at(Num::ZERO);
            let parts = (body, Node::new(amount));
            let mineral = self.mine().mineral;
            self.spawn_unit_of(mineral, pos, Team::new(2), parts)
        }

        fn hall(&mut self, pos: Position) -> StableId {
            let body = BodyForm::box_sized([Num::int(4), Num::int(2)])
                .unwrap()
                .at(Num::ZERO);
            let parts = (Owner::new(PlayerSlot::new(0)), body);
            let depot = self.shop().depot;
            self.spawn_unit_of(depot, pos, Team::new(0), parts)
        }

        fn gather_at(&mut self, workers: &[StableId], node: StableId) {
            let target = ActionTarget::Unit(node);
            let orders: Vec<(StableId, Action)> = workers
                .iter()
                .map(|&unit| (unit, Action::Slot { slot: 0, target }))
                .collect();
            self.give_orders(0, &orders);
        }

        fn node_amount(&self, node: StableId) -> Option<u32> {
            self.try_part::<Node>(node).map(|node| node.amount())
        }

        fn gather_step(&self, worker: StableId) -> Option<GatherStep> {
            let gatherer = self.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.step)
        }

        fn node_of(&self, worker: StableId) -> Option<StableId> {
            let gatherer = self.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.node.node)
        }

        fn holder(&self, node: StableId) -> Option<StableId> {
            self.part::<Node>(node).holder()
        }
    }

    /// A shop's match: a grunt type to train, a producer type of a queue of 3, a depot and a
    /// forge type for supply and requirements to count, and the gold its players hold.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Shop {
        pub(crate) grunt: UnitType,
        pub(crate) barracks: UnitType,
        pub(crate) depot: UnitType,
        pub(crate) forge: UnitType,
        pub(crate) gold: ResourceId,
    }

    /// A spawn production asked for: what and where, and the player that owns it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Spawned {
        pub(crate) at: SpawnAt,
        pub(crate) owner: Option<PlayerSlot>,
    }

    /// Each spawn a shop's spawner was asked for, in order.
    #[derive(Resource, Debug, Default)]
    pub(crate) struct SpawnLog(pub(crate) Vec<Spawned>);

    /// A yard's match: its depot's build.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Yard {
        pub(crate) build: ActionId,
    }

    /// A mine's match: a worker's gather, and the type of the minerals it gathers at.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Mine {
        pub(crate) gather: ActionId,
        pub(crate) mineral: UnitType,
    }
}
