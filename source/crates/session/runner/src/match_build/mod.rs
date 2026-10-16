use bevy_ecs::world::World;
use campfire_capabilities::{Match, MatchSpec, Release};
use campfire_common::SegmentSeed;
use campfire_declare::Role;
use campfire_mode::{ModeInputs, ModeSetup, OptionPicks};
use campfire_package::ModePackages;
use campfire_script::ScriptBudgets;
use campfire_sim::TickRate;
use campfire_units::Units;

/// A match of a mode, built from its read packages.
#[derive(Debug)]
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
        let budgets = ScriptBudgets::new(manifest.script_limits, players);
        MatchSpec::new(seed, rate, Role::Server(budgets)).set(manifest.capabilities)
    }

    /// Puts into `world`, a match of the spec of `packages`, every package's scripts, the books
    /// at the match's rate, and the mode itself, of the values `picks` gives its options.
    pub(crate) fn install_mode(
        world: &mut World,
        packages: &ModePackages,
        players: u32,
        picks: &OptionPicks,
    ) {
        let ModeInputs { units, load } = Self::books(world, packages);
        let setup = ModeSetup {
            script: packages.mode_script(),
            data: packages.data(),
            picks,
            teams: &packages.manifest().teams,
            players,
            units,
        };
        Release::install_mode(world, setup, load);
    }

    /// The first part of `install_mode`: every package's scripts and the books at the match's
    /// rate in place; what the mode's install takes from them.
    pub(crate) fn books(world: &mut World, packages: &ModePackages) -> ModeInputs {
        packages.compile_scripts(|source| Units::compile(world, source));
        let rate = *world.resource::<TickRate>();
        packages.books(rate).install(world)
    }

    /// A restore target: the match of `packages` for `players` players at `rate`, of the values
    /// `picks` gives the mode's options, with its books and its mode in place and no unit, to
    /// restore a snapshot into, as a session that loads a save, or a verifier that checks a
    /// checkpoint's snapshot, does: the books come from the packages, the state from the
    /// snapshot. Its seed is zero, as the seed is not state.
    pub fn restore_target(
        packages: &ModePackages,
        rate: TickRate,
        players: u32,
        picks: &OptionPicks,
    ) -> Match {
        let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
        Self::restore_target_of(spec, packages, players, picks)
    }

    /// The restore target of `spec`, the spec of `packages` for `players` players, of `picks`.
    fn restore_target_of(
        spec: MatchSpec,
        packages: &ModePackages,
        players: u32,
        picks: &OptionPicks,
    ) -> Match {
        let mut game = Match::new(spec);
        Self::install_mode(&mut game, packages, players, picks);
        game
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Match;
    use campfire_common::SegmentSeed;
    use campfire_log::internals::LogCheck;
    use campfire_mode::{Mode, ModeInputs, OptionPicks};
    use campfire_package::ModePackages;
    use campfire_players::Players;
    use campfire_sim::TickRate;
    use campfire_vision::Vision;

    use crate::match_build::MatchBuild;

    impl MatchBuild {
        /// The match of `packages` at `rate`, for `players` players, with every book in place but
        /// the map's vision grid, and no mode installed, as an arena holds it; it starts
        /// with the match's entity alone, spawns the mode's unit types as the mode does for a
        /// test to spawn, and fails its test on any warning it logs.
        pub fn arena(packages: &ModePackages, rate: TickRate, players: u32) -> Match {
            let log = LogCheck::start();
            let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
            let mut game = Match::new(spec).with_log(log);
            let ModeInputs { units, .. } = Self::books(&mut game, packages);
            Vision::clear_grid(&mut game);
            Players::start_without_mode(&mut game, [], 0);
            Mode::load_units(&mut game, &units);
            game
        }

        /// The restore target `restore_target` builds at the mode's default rate, of the options'
        /// defaults, which fails its test on any warning it logs.
        pub fn test_target(packages: &ModePackages, players: u32) -> Match {
            let log = LogCheck::start();
            let picks = OptionPicks::defaults(&packages.data().options);
            Self::restore_target(packages, Self::rate(packages), players, &picks).with_log(log)
        }

        /// The restore target `test_target` builds, its schedule built with no automatic sync
        /// points, so its build panics on every pair of systems that only a sync point keeps in
        /// order, naming them.
        pub fn test_target_without_sync_points(packages: &ModePackages, players: u32) -> Match {
            let log = LogCheck::start();
            let rate = Self::rate(packages);
            let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
            let picks = OptionPicks::defaults(&packages.data().options);
            let spec = spec.without_sync_points();
            let target = Self::restore_target_of(spec, packages, players, &picks);
            target.with_log(log)
        }

        /// The default tick rate of the mode `packages` holds.
        const fn rate(packages: &ModePackages) -> TickRate {
            TickRate::new(packages.manifest().tick_hz.default())
        }
    }
}
