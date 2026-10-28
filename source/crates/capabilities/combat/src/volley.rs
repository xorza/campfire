use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ActionProgress, ProgressIndexes, ResolveKind};
use campfire_declarations::{Role, Scratch};
use campfire_sim::{DueUnit, SimTick, StableId};
use campfire_units::{Acting, Dead};

/// The attacks that go off this tick, by attacker's stable id: what the toggles' pay, the
/// attack events and the strikes visit, read once as the tick's attacks pay, each of them
/// passing over a unit that lacks what it reads. Not state: the strikes take it.
#[derive(Resource, Debug, Default)]
pub(crate) struct Volley(Vec<GoingOff>);

/// An attack that goes off this tick: its attacker, by stable id and entity, and its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GoingOff {
    pub(crate) attacker: StableId,
    pub(crate) entity: Entity,
    pub(crate) target: StableId,
}

impl Volley {
    /// Reads the living units' attacks whose windup ends this tick, from the index of the
    /// resolves.
    pub(crate) fn collect(
        tick: Res<'_, SimTick>,
        attackers: Query<'_, '_, &ActionProgress, (Without<Dead>, Acting)>,
        (mut indexes, mut taken): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
        mut volley: ResMut<'_, Self>,
    ) {
        debug_assert!(
            volley.0.is_empty(),
            "the last tick's strikes took the volley"
        );
        let now = tick.start();
        taken.clear();
        let progress = |entity| attackers.get(entity).ok();
        indexes.take_resolves(now, ResolveKind::Attack, progress, &mut taken);
        volley.0.extend(taken.iter().map(|&DueUnit { id, entity }| {
            let progress = attackers.get(entity).expect("a due attacker");
            GoingOff {
                attacker: id,
                entity,
                target: progress.attacking().expect("a due attack is under way"),
            }
        }));
        volley.0.sort_unstable();
    }

    /// Whether an attack goes off this tick.
    pub(crate) fn any(volley: Res<'_, Self>) -> bool {
        !volley.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = GoingOff> + '_ {
        self.0.iter().copied()
    }

    /// Takes the attacks, as the strikes visit them.
    pub(crate) fn take(&mut self, into: &mut Vec<GoingOff>) {
        into.clear();
        into.append(&mut self.0);
    }
}

impl Scratch for Volley {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
