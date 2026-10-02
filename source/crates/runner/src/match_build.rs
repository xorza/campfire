use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{Mode, ModeInputs, ModeSetup, ScriptBook, ScriptBudgets, Units};
use campfire_package::ModePackages;
use campfire_sim::{StateRegistry, TickRate};

/// A match of a mode, built from its read packages into a world that `SimUpdate::prepare` set
/// up.
#[derive(Debug)]
pub(crate) struct MatchBuild;

impl MatchBuild {
    /// Builds the match of `packages` for `players` players: the core and the declared
    /// capabilities the release has, every package's scripts, the books at the match's rate, and
    /// the mode itself. A declared capability the release does not have yet installs nothing.
    pub(crate) fn run(
        packages: &ModePackages,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) {
        let ModeInputs { units, books } =
            MatchBuild::books(packages, world, schedule, registry, players);
        let setup = ModeSetup {
            script: packages.mode_script(),
            data: packages.data(),
            map: packages.map(),
            teams: &packages.manifest().teams,
            players,
            unit_types: units.unit_types,
            avatars: units.avatars,
            loadout: units.loadout,
            walkers: packages.walkers(),
        };
        Mode::install(world, schedule, registry, setup, books);
    }

    /// The first part of `run`: the core and the declared capabilities, every package's scripts,
    /// and the books at the match's rate in place, but the mode's own, which it gives back.
    pub(crate) fn books(
        packages: &ModePackages,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> ModeInputs {
        let manifest = packages.manifest();
        let budgets = ScriptBudgets::new(manifest.script_limits, players);
        manifest
            .capabilities
            .install(world, schedule, registry, Some(budgets));
        packages.compile_scripts(|source| Units::compile(world, source));
        let rate = *world.resource::<TickRate>();
        let compiled = world.resource::<ScriptBook>().clone();
        packages.books(rate, &compiled).install(world)
    }
}
