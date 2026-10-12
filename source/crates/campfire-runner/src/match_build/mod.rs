use bevy_ecs::world::World;
use campfire_capabilities::{
    MatchSpec, ModeInputs, ModeSetup, ReleaseWorld, ScriptBudgets, UnitsWorld,
};
use campfire_common::SegmentSeed;
use campfire_package::ModePackages;
use campfire_sim::TickRate;

/// A match of a mode, built from its read packages.
#[derive(Debug)]
pub(crate) struct MatchBuild;

impl MatchBuild {
    /// The spec of the match of `packages` for `players` players at `rate`, drawing from
    /// `seed`: of the mode's declared capabilities, its scripts within the mode's limits. A
    /// declared capability the release does not have yet installs nothing.
    pub(crate) fn spec(
        packages: &ModePackages,
        seed: SegmentSeed,
        rate: TickRate,
        players: u32,
    ) -> MatchSpec {
        let manifest = packages.manifest();
        MatchSpec::new(seed, rate)
            .capabilities(manifest.capabilities)
            .script_budgets(ScriptBudgets::new(manifest.script_limits, players))
    }

    /// Puts into `world`, a match of the spec of `packages`, every package's scripts, the books
    /// at the match's rate, and the mode itself.
    pub(crate) fn install_mode(packages: &ModePackages, world: &mut World, players: u32) {
        let ModeInputs { units, books } = MatchBuild::books(packages, world);
        let setup = ModeSetup {
            script: packages.mode_script(),
            data: packages.data(),
            teams: &packages.manifest().teams,
            players,
            units,
        };
        world.install_mode(setup, books);
    }

    /// The first part of `install_mode`: every package's scripts and the books at the match's
    /// rate in place, but the mode's own, which it gives back.
    pub(crate) fn books(packages: &ModePackages, world: &mut World) -> ModeInputs {
        let set = packages.manifest().capabilities;
        packages.compile_scripts(|source| world.compile_script(source));
        world.insert_resource(packages.script_book().clone());
        let rate = *world.resource::<TickRate>();
        packages.books(rate).install(world, set, true)
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;
