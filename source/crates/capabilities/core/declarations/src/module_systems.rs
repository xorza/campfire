use bevy_ecs::schedule::{InternedSystemSet, IntoScheduleConfigs, Schedule, SystemSet};
use campfire_sim::{SimEdgeSystems, SimSystems};

/// A module's system set: where it runs, stated once, against the steps of the modules it builds
/// on, which publish them as their own sets, so it orders against nothing it does not know. A
/// system names one set of its own module, and orders within its set by `.chain()`; one that only
/// some matches build, as one `InstallCx::server` adds, by `.before` or `.after` a system of its
/// set that every match of its module builds.
pub trait ModuleSystems: SystemSet + Copy {
    /// Every variant.
    const ALL: &'static [Self];

    /// Where it runs.
    fn place(self) -> SetPlace;
}

/// The sets of a module that adds no system.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoSystems {}

impl ModuleSystems for NoSystems {
    const ALL: &'static [Self] = &[];

    fn place(self) -> SetPlace {
        match self {}
    }
}

/// Where a module set runs: in a stage, in a gap between stages, or at a step of a module it
/// builds on; and after or before steps of modules it builds on, so it orders against them alone
/// (02-engine-core.md, Sets order against the steps of the modules they build on).
#[derive(Debug, Clone)]
pub struct SetPlace {
    within: InternedSystemSet,
    after: Vec<InternedSystemSet>,
    before: Vec<InternedSystemSet>,
}

impl SetPlace {
    /// In `stage`, in the order the schedule picks among the sets of its step.
    pub fn stage(stage: SimSystems) -> Self {
        Self::within(stage.intern())
    }

    /// In the gap `edge` names, between two stages.
    pub fn gap(edge: SimEdgeSystems) -> Self {
        Self::within(edge.intern())
    }

    /// At `step`, a step of a module it builds on: in its stage, between the steps that come
    /// before and after it.
    pub fn at(step: impl ModuleSystems) -> Self {
        Self::within(step.intern())
    }

    fn within(within: InternedSystemSet) -> Self {
        Self {
            within,
            after: Vec::new(),
            before: Vec::new(),
        }
    }

    /// After `step`, a step of a module it builds on, or of its own.
    #[must_use]
    pub fn after(mut self, step: impl ModuleSystems) -> Self {
        self.after.push(step.intern());
        self
    }

    /// Before `step`, a step of a module it builds on, or of its own.
    #[must_use]
    pub fn before(mut self, step: impl ModuleSystems) -> Self {
        self.before.push(step.intern());
        self
    }

    /// Puts `set` where it says in `schedule`.
    pub fn configure(self, set: impl SystemSet, schedule: &mut Schedule) {
        let Self {
            within,
            after,
            before,
        } = self;
        let mut config = set.in_set(within);
        for step in after {
            config = config.after(step);
        }
        for step in before {
            config = config.before(step);
        }
        schedule.configure_sets(config);
    }
}
