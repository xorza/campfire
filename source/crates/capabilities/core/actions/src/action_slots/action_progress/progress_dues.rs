use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::system::{ResMut, SystemParam};
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, StableId, TimedKept, TimedProcess, TimedRuns};

use crate::action_slots::action_progress::ActionProgress;
use crate::in_progress::InProgress;

/// The units whose attack or cast under way resolves at a tick: each is due at its resolve
/// (04-capabilities/00-overview.md, One due index for each timed process).
#[derive(Debug)]
pub struct Resolves;

impl TimedProcess for Resolves {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static ActionProgress;
    type Books = ();

    fn due((): &(), progress: &ActionProgress) -> Option<Tick> {
        progress.in_progress()?.resolves_at()
    }
}

/// The kinds of resolve the index of the resolves holds, each taken by a pass of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveKind {
    /// An attack's windup ends, for the volley.
    Attack,
    /// A cast resolves, for the casts.
    Cast,
}

impl ResolveKind {
    /// The kind of the resolve `progress` has due at `now` or before, if one is.
    fn due_by(progress: &ActionProgress, now: Tick) -> Option<Self> {
        match progress.in_progress()? {
            InProgress::Attack { resolves_at, .. } => (resolves_at <= now).then_some(Self::Attack),
            underway => underway.cast_due(now).map(|_| Self::Cast),
        }
    }
}

/// The units whose channel runs, or waits, cut, for its `on_interrupt`: each is due at once, and
/// stays held while its channel runs, as a running channel holds its `hold` in each tick.
#[derive(Debug)]
pub struct ChannelSteps;

impl TimedProcess for ChannelSteps {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static ActionProgress;
    type Books = ();

    fn due((): &(), progress: &ActionProgress) -> Option<Tick> {
        progress.is_channel_due().then_some(Tick::ZERO)
    }
}

/// The units with an attack target or a turret target: each is due at once, and stays held while
/// it has one, as a unit chases its target and starts its attacks in each tick.
#[derive(Debug)]
pub struct AttackTargets;

impl TimedProcess for AttackTargets {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static ActionProgress;
    type Books = ();

    fn due((): &(), progress: &ActionProgress) -> Option<Tick> {
        progress.aimed_at().map(|_| Tick::ZERO)
    }
}

/// The units with an order under way, which the start passes check in each tick: one that waits
/// to start, walks in range, winds up or charges. Each is due at once, and stays held while its
/// order is under way.
#[derive(Debug)]
pub struct OrderSteps;

impl TimedProcess for OrderSteps {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static ActionProgress;
    type Books = ();

    fn due((): &(), progress: &ActionProgress) -> Option<Tick> {
        progress.has_order().then_some(Tick::ZERO)
    }
}

impl OrderSteps {
    /// Forgets the unit whose progress goes, as it despawns, in the index of the orders, where it
    /// would stay held at once while no pass finds it.
    pub(crate) fn forget_gone(
        removed: On<'_, '_, Remove<ActionProgress>>,
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        if let Some(mut index) = index {
            index.forget(removed.entity);
        }
    }
}

/// The indexes of the action progress, as a system that changes a unit's progress takes them.
#[derive(SystemParam, Debug)]
pub struct ProgressIndexes<'w> {
    resolves: ResMut<'w, DueIndex<Resolves>>,
    channels: ResMut<'w, DueIndex<ChannelSteps>>,
    attackers: ResMut<'w, DueIndex<AttackTargets>>,
    orders: ResMut<'w, DueIndex<OrderSteps>>,
}

/// The indexes of the action progress of the unit `id`, of `entity`, which each change of its
/// progress that may make it due pushes it into.
#[derive(Debug)]
pub struct ProgressDues<'a> {
    resolves: &'a mut DueIndex<Resolves>,
    channels: &'a mut DueIndex<ChannelSteps>,
    attackers: &'a mut DueIndex<AttackTargets>,
    orders: &'a mut DueIndex<OrderSteps>,
    id: StableId,
    entity: Entity,
}

impl ProgressIndexes<'_> {
    /// Appends to `due` each unit the index of the channels holds due at `now` or before, its
    /// channel running or cut, by tick, then stable id.
    pub fn channels(&self, now: Tick, due: &mut Vec<DueUnit>) {
        self.channels.peek_due(now, due);
    }

    /// Forgets the unit of `entity`, whose channel ended, in the index of the channels.
    pub fn forget_channel(&mut self, entity: Entity) {
        self.channels.forget(entity);
    }

    /// Appends to `due` each unit the index of the attack targets holds, by stable id, its
    /// target held or dropped since.
    pub fn attackers(&self, due: &mut Vec<DueUnit>) {
        self.attackers.peek_due(Tick::ZERO, due);
    }

    /// Forgets the unit of `entity`, which has no attack target, or is gone, in the index of the
    /// attack targets.
    pub fn forget_attacker(&mut self, entity: Entity) {
        self.attackers.forget(entity);
    }

    /// Appends to `due` each unit the index of the orders holds, by stable id, its order under
    /// way or ended since.
    pub fn orders(&self, due: &mut Vec<DueUnit>) {
        self.orders.peek_due(Tick::ZERO, due);
    }

    /// Forgets the unit of `entity`, which has no order under way, or is gone, in the index of
    /// the orders.
    pub fn forget_order(&mut self, entity: Entity) {
        self.orders.forget(entity);
    }

    /// Moves each unit whose resolve of `kind` is due at `now` or before into `due`, out of the
    /// index of the resolves, by tick, then stable id. `progress` reads a unit's progress, none
    /// for a unit gone or dead, which the index then forgets. A unit whose resolve moved later is
    /// pushed again at it, and one whose resolve ended is forgotten. A unit whose resolve of the
    /// other kind is due stays, for the other kind's pass.
    pub fn take_resolves<'p>(
        &mut self,
        now: Tick,
        kind: ResolveKind,
        progress: impl Fn(Entity) -> Option<&'p ActionProgress>,
        due: &mut Vec<DueUnit>,
    ) {
        let from = due.len();
        let others = |unit: DueUnit| {
            let other = progress(unit.entity).and_then(|held| ResolveKind::due_by(held, now));
            other.is_some_and(|other| other != kind)
        };
        self.resolves.take_due_if(now, |unit| !others(unit), due);
        let mut kept = from;
        for at in from..due.len() {
            let unit = due[at];
            let Some(held) = progress(unit.entity) else {
                continue;
            };
            if ResolveKind::due_by(held, now) == Some(kind) {
                due[kept] = unit;
                kept += 1;
            } else if let Some(resolves_at) = Resolves::due(&(), held) {
                self.resolves.push(resolves_at, unit.id, unit.entity);
            }
        }
        due.truncate(kept);
    }

    /// Holds the unit `id`, of `entity`, due at once again, as a pass that took its resolve
    /// could not run it.
    pub fn hold_resolve(&mut self, id: StableId, entity: Entity) {
        self.resolves.push(Tick::ZERO, id, entity);
    }

    /// The indexes, for the progress of the unit `id`, of `entity`.
    pub fn of(&mut self, id: StableId, entity: Entity) -> ProgressDues<'_> {
        ProgressDues {
            resolves: &mut self.resolves,
            channels: &mut self.channels,
            attackers: &mut self.attackers,
            orders: &mut self.orders,
            id,
            entity,
        }
    }
}

impl ProgressDues<'_> {
    /// Pushes the unit as `progress` makes it due: at its resolve, and its channel, its attack
    /// target and its order at once; a unit whose order ended leaves the index of the orders.
    pub fn push(&mut self, progress: &ActionProgress) {
        if let Some(at) = Resolves::due(&(), progress) {
            self.resolves.push(at, self.id, self.entity);
        }
        if let Some(at) = ChannelSteps::due(&(), progress) {
            self.channels.push(at, self.id, self.entity);
        }
        if let Some(at) = AttackTargets::due(&(), progress) {
            self.attackers.push(at, self.id, self.entity);
        }
        match OrderSteps::due(&(), progress) {
            Some(at) => self.orders.push(at, self.id, self.entity),
            None => self.orders.forget(self.entity),
        }
    }

    /// The same indexes, for a shorter borrow, as a view of the unit's slots takes them.
    pub const fn reborrow(&mut self) -> ProgressDues<'_> {
        ProgressDues {
            resolves: self.resolves,
            channels: self.channels,
            attackers: self.attackers,
            orders: self.orders,
            id: self.id,
            entity: self.entity,
        }
    }

    /// Runs `change` on `world`, with the indexes the progress of the unit `id`, of `entity`,
    /// pushes it into, as an exclusive system or an effect changes the unit.
    pub fn with<R>(
        world: &mut World,
        id: StableId,
        entity: Entity,
        change: impl FnOnce(&mut World, &mut ProgressDues<'_>) -> R,
    ) -> R {
        world.resource_scope(|world, mut resolves: Mut<'_, DueIndex<Resolves>>| {
            world.resource_scope(|world, mut channels: Mut<'_, DueIndex<ChannelSteps>>| {
                world.resource_scope(|world, mut attackers: Mut<'_, DueIndex<AttackTargets>>| {
                    world.resource_scope(|world, mut orders: Mut<'_, DueIndex<OrderSteps>>| {
                        let mut dues = ProgressDues {
                            resolves: &mut resolves,
                            channels: &mut channels,
                            attackers: &mut attackers,
                            orders: &mut orders,
                            id,
                            entity,
                        };
                        change(world, &mut dues)
                    })
                })
            })
        })
    }

    /// Runs `change` on the progress of the unit `id`, of `entity`, in `world`, as `with` does;
    /// none for a unit that has no progress.
    pub fn change<R>(
        world: &mut World,
        id: StableId,
        entity: Entity,
        change: impl FnOnce(&mut ActionProgress, &mut ProgressDues<'_>) -> R,
    ) -> Option<R> {
        Self::with(world, id, entity, |world, dues| {
            let mut progress = world.get_mut::<ActionProgress>(entity)?;
            Some(change(&mut progress, dues))
        })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_sim::{DueIndex, SimWorld, StableId};

    use crate::action_slots::action_progress::ActionProgress;
    use crate::action_slots::action_progress::progress_dues::{
        AttackTargets, ChannelSteps, OrderSteps, ProgressDues, Resolves,
    };

    /// Indexes of no match, for a progress a test makes and checks outside one.
    #[derive(Debug, Default)]
    pub struct LooseDues {
        resolves: DueIndex<Resolves>,
        channels: DueIndex<ChannelSteps>,
        attackers: DueIndex<AttackTargets>,
        orders: DueIndex<OrderSteps>,
    }

    impl LooseDues {
        /// The indexes, for a progress of no unit: what they hold is never read, an entry for the
        /// first entity alone, as an index keeps a place for each entity index below the largest.
        pub const fn dues(&mut self) -> ProgressDues<'_> {
            ProgressDues {
                resolves: &mut self.resolves,
                channels: &mut self.channels,
                attackers: &mut self.attackers,
                orders: &mut self.orders,
                id: StableId::MATCH,
                entity: Entity::from_raw_u32(0).unwrap(),
            }
        }
    }

    impl ProgressDues<'_> {
        /// Runs `change` on the progress of the unit `id` in `world`, as a test changes it in
        /// place; panics when the unit is gone or has no progress.
        pub fn change_unit<R>(
            world: &mut World,
            id: StableId,
            change: impl FnOnce(&mut ActionProgress, &mut ProgressDues<'_>) -> R,
        ) -> R {
            let entity = world.entity_of(id).expect("a unit in the match");
            Self::change(world, id, entity, change).expect("a unit with its progress")
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::system::{Query, RunSystemOnce};
    use campfire_math::Vec3;
    use campfire_sim::IdAllocator;
    use campfire_values::ActionStart;

    use super::*;
    use crate::action_slots::action_progress::attack_target::AttackTarget;
    use crate::action_target::ActionTarget;
    use crate::internals::LooseDues;

    #[test]
    fn each_pass_takes_its_kind_of_resolve_and_leaves_the_other_kinds() {
        let mut world = World::new();
        world.init_resource::<DueIndex<Resolves>>();
        world.init_resource::<DueIndex<ChannelSteps>>();
        world.init_resource::<DueIndex<AttackTargets>>();
        world.init_resource::<DueIndex<OrderSteps>>();
        let mut ids = IdAllocator::default();
        let [attack, cast, moved, gone, ordered] = [(); 5].map(|()| ids.allocate());
        let loose = &mut LooseDues::default();
        let mut attacking = ActionProgress::default();
        attacking.set_attack_target(Some(AttackTarget::ordered(cast)), &mut loose.dues());
        attacking.schedule_attack(0, Tick::new(2), &mut loose.dues());
        let mut waiting = ActionProgress::default();
        waiting.order(0, ActionTarget::None, &mut loose.dues());
        let mut started = |resolves_at| {
            let mut progress = ActionProgress::default();
            progress.order(0, ActionTarget::None, &mut loose.dues());
            let start = ActionStart {
                origin: Vec3::ZERO,
                charge: None,
            };
            progress.schedule(
                Tick::new(resolves_at),
                ActionTarget::None,
                start,
                &mut loose.dues(),
            );
            progress
        };
        // Each unit is held at tick 2, or 1; `moved`'s cast resolves at 5 since, `gone` is
        // despawned, and `ordered` waits for its start, with no resolve.
        let units = [
            (attack, attacking, 2),
            (cast, started(2), 2),
            (moved, started(5), 2),
            (gone, started(2), 1),
            (ordered, waiting, 1),
        ];
        let entities = units.map(|(id, progress, held)| {
            let entity = world.spawn(progress).id();
            let at = Tick::new(held);
            world
                .resource_mut::<DueIndex<Resolves>>()
                .push(at, id, entity);
            entity
        });
        world.despawn(entities[3]);
        let take = |world: &mut World, kind| {
            world
                .run_system_once(
                    move |mut indexes: ProgressIndexes<'_>,
                          units: Query<'_, '_, &ActionProgress>| {
                        let mut due = Vec::new();
                        let progress = |entity| units.get(entity).ok();
                        indexes.take_resolves(Tick::new(2), kind, progress, &mut due);
                        due.iter().map(|unit| unit.id).collect::<Vec<_>>()
                    },
                )
                .unwrap()
        };
        // The casts take `cast`, push `moved` again at 5, forget `gone` and `ordered`, and leave
        // `attack` to the volley, which takes it.
        assert_eq!(take(&mut world, ResolveKind::Cast), [cast]);
        let index = world.resource::<DueIndex<Resolves>>();
        let held = [0, 2, 3, 4].map(|at| index.due_at(entities[at]));
        assert_eq!(held, [Some(Tick::new(2)), Some(Tick::new(5)), None, None]);
        assert_eq!(take(&mut world, ResolveKind::Attack), [attack]);
        let index = world.resource::<DueIndex<Resolves>>();
        assert_eq!(
            (index.len(), index.due_at(entities[2])),
            (1, Some(Tick::new(5)))
        );
    }

    #[test]
    fn an_order_holds_its_unit_in_the_index_of_the_orders_until_it_ends() {
        let mut world = World::new();
        world.init_resource::<DueIndex<Resolves>>();
        world.init_resource::<DueIndex<ChannelSteps>>();
        world.init_resource::<DueIndex<AttackTargets>>();
        world.init_resource::<DueIndex<OrderSteps>>();
        let mut ids = IdAllocator::default();
        let id = ids.allocate();
        let entity = world.spawn(ActionProgress::default()).id();
        let change = |world: &mut World, change: fn(&mut ActionProgress, &mut ProgressDues<'_>)| {
            ProgressDues::change(world, id, entity, change).unwrap();
            world.resource::<DueIndex<OrderSteps>>().due_at(entity)
        };
        // An attack target alone is no order; an order is held at once, through its windup, and
        // back to its order by an interrupt, until it ends.
        assert_eq!(
            change(&mut world, |progress, dues| {
                progress.set_attack_target(Some(AttackTarget::ordered(StableId::MATCH)), dues);
            }),
            None
        );
        let ordered = |progress: &mut ActionProgress, dues: &mut ProgressDues<'_>| {
            progress.order(0, ActionTarget::None, dues);
        };
        assert_eq!(change(&mut world, ordered), Some(Tick::ZERO));
        let started = |progress: &mut ActionProgress, dues: &mut ProgressDues<'_>| {
            let start = ActionStart {
                origin: Vec3::ZERO,
                charge: None,
            };
            progress.schedule(Tick::new(4), ActionTarget::None, start, dues);
        };
        assert_eq!(change(&mut world, started), Some(Tick::ZERO));
        assert_eq!(
            change(&mut world, ActionProgress::interrupt),
            Some(Tick::ZERO)
        );
        assert_eq!(change(&mut world, ActionProgress::end), None);
    }
}
