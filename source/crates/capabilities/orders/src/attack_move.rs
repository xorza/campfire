use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Has, With, Without};
use bevy_ecs::system::{Local, Query, ResMut};
use bevy_ecs::world::World;
use campfire_actions::ActionProgress;
use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::{DueIndex, DueUnit, SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{Dead, Destination, UnitEntity, Walk};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// Where a unit that scans attack-moves to, a part of each unit of a type with an idle scan from
/// its spawn: none until an attack-move order gives a goal, which every other order drops; and
/// whether its scan stopped its walk to attack. While it holds a goal, its scan looks as it
/// walks, and once the scan's target is gone it walks on (04-capabilities/control.md, An army
/// attack-moves).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackMove {
    goal: Option<Vec3>,
    stopped: bool,
}

impl AttackMove {
    /// No attack-move under way.
    pub const NONE: Self = Self {
        goal: None,
        stopped: false,
    };

    /// The attack-move to `goal`, or none, walking.
    pub(crate) const fn to(goal: Option<Vec3>) -> Self {
        Self {
            goal,
            stopped: false,
        }
    }

    /// Its goal, while one is under way.
    pub const fn goal(self) -> Option<Vec3> {
        self.goal
    }

    /// Marks the walk stopped by the scan, which took a target.
    pub(crate) const fn stop(&mut self) {
        self.stopped = true;
    }

    /// For each living unit of the index that attack-moves and stands, with no target and
    /// nothing under way: one its scan stopped walks on to its goal, as its target died or left
    /// its reach; any other ended its walk, at its goal, short of it or where a body kept it, and
    /// ends its attack-move. A unit that died since the last run drops its goal, so it walks
    /// nowhere when it lives again. The index forgets each unit whose goal ended, and each that is
    /// dead, gone or walks not.
    pub(super) fn walk_on(
        mut units: Query<'_, '_, (&mut Self, &ActionProgress, Walk), Without<Dead>>,
        mut dead: Query<'_, '_, &mut Self, (Added<Dead>, With<Self>)>,
        (mut index, mut held): (ResMut<'_, DueIndex<Self>>, Local<'_, Vec<DueUnit>>),
    ) {
        for mut attack_move in &mut dead {
            attack_move.set_if_neq(Self::NONE);
        }
        held.clear();
        index.peek_due(Tick::ZERO, &mut held);
        for &DueUnit { entity, .. } in &*held {
            let Ok((mut attack_move, progress, mut walk)) = units.get_mut(entity) else {
                index.forget(entity);
                continue;
            };
            let Some(goal) = attack_move.goal else {
                index.forget(entity);
                continue;
            };
            let stands = walk.goal().is_none()
                && progress.attack_target().is_none()
                && progress.in_progress().is_none();
            if !stands {
                continue;
            }
            if attack_move.stopped {
                attack_move.set_if_neq(Self::to(Some(goal)));
                walk.to(Some(goal));
            } else {
                attack_move.set_if_neq(Self::NONE);
                index.forget(entity);
            }
        }
    }
}

/// The living units that walk and attack-move: each is due at once, and stays held while it has
/// a goal, as `walk_on` checks in each tick whether it stands.
impl TimedProcess for AttackMove {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Has<Destination>, Has<Dead>);
    type Books = ();

    fn due((): &(), (attack_move, walks, dead): (&Self, bool, bool)) -> Option<Tick> {
        attack_move
            .goal
            .filter(|_| walks && !dead)
            .map(|_| Tick::ZERO)
    }
}

impl SimComponent for AttackMove {
    const NAME: &'static str = "orders.attack_move";
    type Host = UnitEntity;

    // Any goal a unit walks to is one its order gave.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

/// Public, as the destination it walks to is, and sent predicted, as a predicting client gives
/// its own units' orders.
impl Replication for AttackMove {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
