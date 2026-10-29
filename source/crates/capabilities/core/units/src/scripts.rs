use bevy_ecs::change_detection::CheckChangeTicks;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::{NonSend, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::EnumAll;
use campfire_declarations::{Declarations, InstallCx, MatchFacts, Module, ModuleSystems, SetPlace};
use campfire_names::MatchNames;
use campfire_script::{
    CompileError, CoreApi, Ctx, ScriptBudgets, ScriptFailures, ScriptHost, ScriptId, ScriptPrints,
    View, ViewApi,
};
use campfire_sim::{Capability, MatchRate, SimTick};

use crate::units::UnitsSystems;
use crate::view_reader::ViewReader;

/// The scripts' steps of each tick, within the core's first and last.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ScriptsSystems {
    /// The tick begins: every budget starts full, and the last tick's failures clear.
    Begin,
    /// The tick closes: what the scripts printed reaches its report.
    Close,
}

impl ModuleSystems for ScriptsSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Begin => SetPlace::at(UnitsSystems::Begin),
            Self::Close => SetPlace::at(UnitsSystems::Close),
        }
    }
}

/// The scripts' runtime under every capability's scripts: the match's names, the script host
/// they run in within the role's budgets, the context of their calls, the units as scripts see
/// them with the queries on them, and each tick's budgets, failures and prints. Every match
/// installs it first among the core, as each module's install binds its script API into its host;
/// a client runs no scripts, and its bot reads the world through its own view.
#[derive(Debug)]
pub struct Scripts;

impl Module for Scripts {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = ScriptsSystems;

    fn declare<D: Declarations>(d: &mut D) {
        // Before every book, which shares its names into it as it installs.
        d.package_data::<MatchNames>();
        d.cache::<ViewReader>();
        d.non_send::<View>(|world, role| {
            let rate = *world.resource::<MatchRate>();
            let visibility = world.resource::<MatchFacts>().visibility;
            role.scripts()
                .then(|| View::new(rate, visibility, ViewReader::read))
        });
        d.non_send::<Ctx>(|world, role| {
            role.scripts()
                .then(|| Ctx::new(world.non_send::<View>().clone()))
        });
        d.non_send::<ScriptHost>(|world, role| {
            let setup = role.budgets()?;
            let mut host = ScriptHost::new(setup.limits.per_call);
            world.non_send::<Ctx>().tag(&mut host);
            Some(host)
        });
        d.report::<ScriptBudgets>();
        d.report::<ScriptPrints>();
        d.report::<ScriptFailures>();
        d.observer(Self::refill_view);
        d.observer(ViewReader::disabled);
        d.observer(ViewReader::enabled);
        d.api(CoreApi::register);
        d.api(ViewApi::register_unit);
        d.api(ViewApi::register_queries);
    }

    /// Adds the runtime's steps to a match that runs scripts: in Inputs, every budget starts full
    /// and the last tick's failures clear; as the tick closes, what the scripts printed reaches
    /// its report.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.scripted(ScriptsSystems::Begin, Self::begin_tick);
        cx.scripted(ScriptsSystems::Close, Self::report_prints);
    }
}

impl Scripts {
    /// Fills every script budget, and clears the failures the last tick recorded, as each tick
    /// starts.
    fn begin_tick(
        mut budgets: ResMut<'_, ScriptBudgets>,
        mut failures: ResMut<'_, ScriptFailures>,
    ) {
        budgets.refill();
        failures.clear();
    }

    /// Compiles `source` in the match's script host, once for every capability that runs it.
    pub fn compile(world: &mut World, source: &str) -> Result<ScriptId, CompileError> {
        world.non_send_mut::<ScriptHost>().compile(source)
    }

    /// Moves what the scripts printed into the report, as each tick ends, after its last call.
    fn report_prints(
        tick: Res<'_, SimTick>,
        host: NonSend<'_, ScriptHost>,
        mut prints: ResMut<'_, ScriptPrints>,
    ) {
        host.take_prints(tick.start(), &mut prints);
    }

    /// The observer of Bevy's check of change ticks: the script view's next read fills every
    /// row, as its queries of changed parts may miss a change after the check; a match that runs
    /// no scripts has no view.
    fn refill_view(_: On<'_, '_, CheckChangeTicks>, view: Option<NonSend<'_, View>>) {
        if let Some(view) = view {
            view.refill_next();
        }
    }
}
