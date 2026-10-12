use bevy_ecs::world::World;
use campfire_capabilities::{MatchSpec, ModeInputs, ModeSetup, Release, ScriptBudgets, Units};
use campfire_common::SegmentSeed;
use campfire_package::ModePackages;
use campfire_sim::TickRate;

/// A match of a mode, built from its read packages.
#[derive(Debug)]
#[cfg_attr(
    not(feature = "internals"),
    expect(
        unreachable_pub,
        reason = "the internals facade exports it, for its restore targets"
    )
)]
pub struct MatchBuild;

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
        Release::install_mode(world, setup, books);
    }

    /// The first part of `install_mode`: every package's scripts and the books at the match's
    /// rate in place, but the mode's own, which it gives back.
    pub(crate) fn books(packages: &ModePackages, world: &mut World) -> ModeInputs {
        let set = packages.manifest().capabilities;
        packages.compile_scripts(|source| Units::compile(world, source));
        world.insert_resource(packages.script_book().clone());
        let rate = *world.resource::<TickRate>();
        packages.books(rate).install(world, set, true)
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::internals::Players;
    use campfire_capabilities::{Match, MatchSpec, ModeInputs};
    use campfire_common::SegmentSeed;
    use campfire_log::internals::LogCheck;
    use campfire_package::ModePackages;
    use campfire_sim::TickRate;

    use crate::match_build::MatchBuild;

    /// The seed of a match whose randomness no test reads.
    const ZERO_SEED: SegmentSeed = SegmentSeed::new([0; 32]);

    impl MatchBuild {
        /// The match of `packages` at `rate`, for `players` players, with every book in place,
        /// the mode's own among them, and no mode installed, as an arena holds it; it starts
        /// with the match's entity alone, and fails its test on any warning it logs.
        pub(crate) fn arena(packages: &ModePackages, rate: TickRate, players: u32) -> Match {
            let log = LogCheck::start();
            let spec = MatchBuild::spec(packages, ZERO_SEED, rate, players);
            let mut game = Match::new(spec).with_log(log);
            let ModeInputs { books, .. } = MatchBuild::books(packages, &mut game);
            books.install(&mut game);
            Players::start_without_mode(&mut game, [], 0);
            game
        }

        /// A restore target: the match of `packages` for `players` players at the mode's
        /// default rate, with its books in place and no unit, to restore snapshots into, as a
        /// session that loads a save will: the books come from the packages, the state from the
        /// snapshot. It fails its test on any warning it logs.
        pub fn restore_target(packages: &ModePackages, players: u32) -> Match {
            let log = LogCheck::start();
            let spec = MatchBuild::spec(packages, ZERO_SEED, MatchBuild::rate(packages), players);
            MatchBuild::restore_target_of(log, packages, players, spec)
        }

        /// The restore target `restore_target` builds, its schedule built with no automatic sync
        /// points, so its build panics on every pair of systems that only a sync point keeps in
        /// order, naming them.
        pub fn restore_target_without_sync_points(packages: &ModePackages, players: u32) -> Match {
            let log = LogCheck::start();
            let spec = MatchBuild::spec(packages, ZERO_SEED, MatchBuild::rate(packages), players);
            MatchBuild::restore_target_of(log, packages, players, spec.without_sync_points())
        }

        /// The default tick rate of the mode `packages` holds.
        fn rate(packages: &ModePackages) -> TickRate {
            TickRate::new(packages.manifest().tick_hz.default())
        }

        /// A restore target of `spec`, the spec of `packages` for `players` players, which fails
        /// its test on any warning `log` takes.
        fn restore_target_of(
            log: LogCheck,
            packages: &ModePackages,
            players: u32,
            spec: MatchSpec,
        ) -> Match {
            let mut game = Match::new(spec).with_log(log);
            MatchBuild::install_mode(packages, &mut game, players);
            game
        }
    }
}
