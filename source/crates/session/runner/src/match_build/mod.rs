use bevy_ecs::world::World;
use campfire_capabilities::{Match, MatchKind, MatchSetup, ModeStart, Release};
use campfire_common::SegmentSeed;
use campfire_declarations::Role;
use campfire_mode::{HostPicks, ModeSetup};
use campfire_package::ModePackages;
use campfire_script::ScriptPools;
use campfire_sim::MatchRate;

/// A match of a mode, built from its read packages.
#[derive(Debug)]
pub struct MatchBuild;

impl MatchBuild {
    /// The spec of the match of `packages` for `players` players at `rate`, drawing from
    /// `seed`: its scripts within the mode's limits. Its capabilities are its books', the mode's
    /// declared ones, of which one the release does not have yet installs nothing.
    pub(crate) fn spec(
        packages: &ModePackages,
        seed: SegmentSeed,
        rate: MatchRate,
        players: u32,
    ) -> MatchSetup {
        let manifest = packages.manifest();
        let pools = ScriptPools {
            limits: manifest.script_limits,
            players,
        };
        MatchSetup::new(seed, rate, Role::Server(pools))
    }

    /// Builds the match of `spec` in `world`, a new world, of `packages`, for `players` players,
    /// with every package's scripts, the books at the match's rate, and the mode itself, of the
    /// values `picks` gives its options and its player slots' choices.
    pub(crate) fn build(
        world: &mut World,
        match_setup: MatchSetup,
        packages: &ModePackages,
        players: u32,
        picks: &HostPicks,
    ) {
        let books = packages.books(match_setup.rate());
        let scripts = packages.scripts();
        let setup = ModeSetup {
            scripts: packages.mode_scripts(),
            data: packages.data(),
            picks,
            teams: &packages.manifest().teams,
            players,
        };
        let start = ModeStart {
            scripts: &scripts,
            setup,
        };
        Release::build(world, match_setup, books, MatchKind::Mode(start));
    }

    /// A restore target: the match of `packages` for `players` players at `rate`, of the values
    /// `picks` gives the mode's options, with its books and its mode in place and no unit, to
    /// restore a snapshot into, as a session that loads a save, or a verifier that checks a
    /// checkpoint's snapshot, does: the books come from the packages, the state from the
    /// snapshot. Its seed is zero, as the seed is not state.
    pub fn restore_target(
        packages: &ModePackages,
        rate: MatchRate,
        players: u32,
        picks: &HostPicks,
    ) -> Match {
        let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
        Self::restore_target_of(spec, packages, players, picks)
    }

    /// The restore target of `setup`, the setup of `packages` for `players` players, of `picks`.
    fn restore_target_of(
        setup: MatchSetup,
        packages: &ModePackages,
        players: u32,
        picks: &HostPicks,
    ) -> Match {
        let mut world = World::new();
        Self::build(&mut world, setup, packages, players, picks);
        Match::from_built(world)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Match;
    use campfire_common::SegmentSeed;
    use campfire_log::internals::LogCheck;
    use campfire_mode::HostPicks;
    use campfire_package::ModePackages;
    use campfire_sim::MatchRate;

    use crate::match_build::MatchBuild;

    impl MatchBuild {
        /// The restore target `restore_target` builds at the mode's default rate, of the options'
        /// defaults, which fails its test on any warning it logs.
        pub fn test_target(packages: &ModePackages, players: u32) -> Match {
            let player_slots = usize::try_from(players).expect("a player count fits usize");
            let picks = HostPicks::defaults(packages.data(), player_slots);
            Self::test_target_picked(packages, players, &picks)
        }

        /// The restore target `test_target` builds, of what the host picked, `picks`: the mode
        /// installed and not started, as a session's build makes it.
        pub fn test_target_picked(
            packages: &ModePackages,
            players: u32,
            picks: &HostPicks,
        ) -> Match {
            let log = LogCheck::start();
            Self::restore_target(packages, Self::rate(packages), players, picks).with_log(log)
        }

        /// The restore target `test_target` builds, its schedule built with no automatic sync
        /// points, so its build panics on every pair of systems that only a sync point keeps in
        /// order, naming them.
        pub fn test_target_without_sync_points(packages: &ModePackages, players: u32) -> Match {
            let log = LogCheck::start();
            let rate = Self::rate(packages);
            let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
            let player_slots = usize::try_from(players).expect("a player count fits usize");
            let picks = HostPicks::defaults(packages.data(), player_slots);
            let spec = spec.without_sync_points();
            let target = Self::restore_target_of(spec, packages, players, &picks);
            target.with_log(log)
        }

        /// The default tick rate of the mode `packages` holds.
        fn rate(packages: &ModePackages) -> MatchRate {
            MatchRate::new(packages.manifest().tick_hz.default())
        }
    }
}
