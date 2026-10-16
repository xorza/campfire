use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_actions::ActionProgress;
use campfire_common::Tick;
use campfire_declare::Scratch;
use campfire_sim::{SimTick, StableId};
use campfire_units::Dead;

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
    /// Reads the living units' attacks whose windup ends this tick.
    pub(crate) fn collect(
        tick: Res<'_, SimTick>,
        attackers: Query<'_, '_, (Entity, &StableId, &ActionProgress), Without<Dead>>,
        mut volley: ResMut<'_, Self>,
    ) {
        debug_assert!(
            volley.0.is_empty(),
            "the last tick's strikes took the volley"
        );
        let now = tick.start();
        volley.0.extend(
            attackers
                .iter()
                .filter_map(|(entity, &attacker, progress)| {
                    Some(GoingOff {
                        attacker,
                        entity,
                        target: Self::target(progress, now)?,
                    })
                }),
        );
        volley.0.sort_unstable();
    }

    /// Whether an attack goes off this tick.
    pub(crate) fn any(volley: Res<'_, Self>) -> bool {
        !volley.0.is_empty()
    }

    /// The target of the attack `progress` has under way whose windup ends by `now`, if one does.
    pub(crate) fn target(progress: &ActionProgress, now: Tick) -> Option<StableId> {
        let target = progress.attacking()?;
        let resolves_at = progress.in_progress()?.resolves_at()?;
        (resolves_at <= now).then_some(target)
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
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
