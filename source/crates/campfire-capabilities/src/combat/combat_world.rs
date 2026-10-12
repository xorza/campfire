use bevy_ecs::world::World;

use crate::stats::life_pool::LifePool;
use crate::stats::pool_id::PoolId;

/// What the combat module gives a world, for every crate.
pub trait CombatWorld {
    /// The match's life pool; `None` when its mode names none.
    fn life_pool(&self) -> Option<PoolId>;
}

impl CombatWorld for World {
    fn life_pool(&self) -> Option<PoolId> {
        Some(self.get_resource::<LifePool>()?.0)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::StableId;

    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::pass_queue::PassQueue;
    use crate::units::view::View;

    /// What the combat module gives a world for the tests and benches of every crate.
    pub trait CombatWorldInternals {
        /// Queues `amount` of the damage kind `kind` from `source` to `target` for the tick's damage
        /// pass, as an effect of no action deals it: for a test that deals what no input can.
        fn queue_damage(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: Num,
            kind: &str,
        );
    }

    impl CombatWorldInternals for World {
        fn queue_damage(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: Num,
            kind: &str,
        ) {
            let kind = self
                .non_send::<View>()
                .damage_kind_named(kind)
                .unwrap_or_else(|| panic!("the mode declares the damage kind {kind}"));
            self.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount,
                kind,
                cause: DamageCause::Effect,
                action: None,
                depth: 0,
                hit: None,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_common::Tick;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, StableId};

    use crate::actions::action_book::ActionBook;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_times::SlotTimes;
    use crate::combat::combat_bindings::CombatBindings;
    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::damage_pass::DamagePass;
    use crate::combat::internals::Armed;
    use crate::combat::pass_queue::PassQueue;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::stats::life_pool::LifePool;
    use crate::stats::pool_id::PoolId;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;

    /// What the combat module gives a world for its own tests.
    pub(crate) trait CombatWorldTest {
        /// Heals `id` at once, as the pass heals it, when it exists and its life is above zero.
        fn heal_unit(&mut self, id: StableId, amount: Num);

        /// Binds `life` as the life pool, with no stat bound, as a test world with no mode needs.
        fn bind_life(&mut self, life: PoolId);

        fn spawn_armed(&mut self, team: Team, pos: Position, combatant: Armed) -> StableId;

        fn spawn_armed_of(
            &mut self,
            unit_type: UnitType,
            team: Team,
            pos: Position,
            combatant: Armed,
        ) -> StableId;

        fn attack(&mut self, attacker: StableId, target: StableId);

        /// The attack of unit `id`, whose one weapon sits in its first slot.
        fn state(&self, id: StableId) -> Attack;

        /// Gives `id` the values of `load_damage_stats`' stats.
        fn stats(&mut self, id: StableId, values: [Num; 3]);

        /// Queues `amount` of damage from `source` to `target`, dealt by `cause`.
        fn damage(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            cause: DamageCause,
        );
    }

    impl CombatWorldTest for World {
        fn heal_unit(&mut self, id: StableId, amount: Num) {
            if let Some(entity) = DamagePass::living(self, id) {
                DamagePass::heal(self, entity, amount);
            }
        }

        fn bind_life(&mut self, life: PoolId) {
            self.insert_resource(LifePool(life));
            self.insert_resource(CombatBindings::UNBOUND);
        }

        fn spawn_armed(&mut self, team: Team, pos: Position, combatant: Armed) -> StableId {
            let unit_type = self.test_unit_type();
            self.spawn_armed_of(unit_type, team, pos, combatant)
        }

        fn spawn_armed_of(
            &mut self,
            unit_type: UnitType,
            team: Team,
            pos: Position,
            combatant: Armed,
        ) -> StableId {
            let bundle = combatant.bundle(self);
            self.spawn_unit_of(unit_type, pos, team, bundle)
        }

        fn attack(&mut self, attacker: StableId, target: StableId) {
            let entity = self.unit_entity(attacker);
            self.get_mut::<ActionSlots>(entity)
                .unwrap()
                .set_attack_target(Some(target));
        }

        fn state(&self, id: StableId) -> Attack {
            let slots = self.try_part::<ActionSlots>(id).unwrap();
            let slot = slots.slot(0).unwrap();
            let time = self.try_part::<SlotTimes>(id).unwrap().slot(0).unwrap();
            let book = self.resource::<ActionBook>();
            let windup = book.get(slot.action.unwrap()).unwrap().ranks[0].windup;
            let started = slots.attacking().and(slots.in_progress());
            let started = started.and_then(InProgress::resolves_at);
            Attack {
                target: slots.attack_target(),
                started: started.map(|at| Tick::new(at.get() - windup.get())),
                ready_at: time.ready_at,
            }
        }

        fn stats(&mut self, id: StableId, values: [Num; 3]) {
            let mut stats = UnitStats::default();
            stats.refill().extend(values);
            self.insert_parts(id, stats);
        }

        fn damage(
            &mut self,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            cause: DamageCause,
        ) {
            self.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind: DamageKind::new(0),
                cause,
                action: None,
                depth: 0,
                hit: None,
            });
        }
    }

    /// A unit's attack as these tests read it: its target, the tick the attack in its windup
    /// started in, and the first tick its weapon may attack again.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct Attack {
        target: Option<StableId>,
        started: Option<Tick>,
        ready_at: Tick,
    }

    impl Attack {
        pub(crate) const fn target(self) -> Option<StableId> {
            self.target
        }

        pub(crate) const fn started(self) -> Option<Tick> {
            self.started
        }

        pub(crate) const fn ready_at(self) -> Tick {
            self.ready_at
        }
    }
}
