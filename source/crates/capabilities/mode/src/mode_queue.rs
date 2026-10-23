use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs};
use bevy_ecs::system::ScheduleSystem;
use campfire_declarations::InstallCx;
use campfire_script::{EventPhase, HookSet, ModeEvents};

use crate::ModeSystems;
use crate::mode_hooks::ModeHooks;

/// A queue of events a module declares for the mode's scripts to answer, as the mode installs its
/// answer: in a mode whose script defines one of its hooks the answer, else the drop, each after
/// the queue declared before it.
#[derive(Debug, Clone, Copy)]
pub struct ModeQueue {
    phase: EventPhase,
    add: fn(&mut InstallCx<'_, ModeSystems>, &HookSet, Option<Self>),
    after: fn(ScheduleConfigs<ScheduleSystem>) -> ScheduleConfigs<ScheduleSystem>,
}

/// The queues the modules of a match declare, by phase, each phase's in the order they declare
/// them.
#[derive(Resource, Debug, Default)]
pub struct ModeQueues(Vec<ModeQueue>);

impl ModeQueue {
    /// The queue `Q`, which answers in `phase`.
    pub fn of<Q: ModeEvents>(phase: EventPhase) -> Self {
        Self {
            phase,
            add: |cx, hooks, before| {
                let system = if Q::HOOKS.iter().any(|&hook| hooks.contains(hook)) {
                    ModeHooks::answer::<Q>.into_configs()
                } else {
                    ModeHooks::drop::<Q>.into_configs()
                };
                let system = match before {
                    Some(before) => (before.after)(system),
                    None => system,
                };
                cx.add(ModeSystems::Events, system);
            },
            after: |system| {
                system
                    .after(ModeHooks::answer::<Q>)
                    .after(ModeHooks::drop::<Q>)
            },
        }
    }

    /// Adds the queue's answer, or its drop, to the mode's events, after `before`'s, `hooks`
    /// being those the mode's scripts define.
    pub(crate) fn add(
        self,
        cx: &mut InstallCx<'_, ModeSystems>,
        hooks: &HookSet,
        before: Option<Self>,
    ) {
        (self.add)(cx, hooks, before);
    }
}

impl ModeQueues {
    /// Adds `queue` after those of its phase and those before it.
    pub fn push(&mut self, queue: ModeQueue) {
        let at = self.0.partition_point(|held| held.phase <= queue.phase);
        self.0.insert(at, queue);
    }

    /// The queues, by phase, then in the order they were declared.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = ModeQueue> + '_ {
        self.0.iter().copied()
    }
}
