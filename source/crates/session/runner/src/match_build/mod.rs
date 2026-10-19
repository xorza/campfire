use bevy_ecs::world::World;
use campfire_capabilities::{Match, MatchKind, MatchSetup, ModeStart, Release};
use campfire_common::SegmentSeed;
use campfire_declare::Role;
use campfire_mode::{ModeSetup, OptionPicks};
use campfire_package::ModePackages;
use campfire_script::ScriptPools;
use campfire_sim::MatchRate;

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
        rate: MatchRate,
        players: u32,
    ) -> MatchSetup {
        let manifest = packages.manifest();
        let pools = ScriptPools {
            limits: manifest.script_limits,
            players,
        };
        MatchSetup::new(seed, rate, Role::Server(pools)).set(manifest.capabilities)
    }

    /// Builds the match of `spec` in `world`, a new world, of `packages`, for `players` players,
    /// with every package's scripts, the books at the match's rate, and the mode itself, of the
    /// values `picks` gives its options.
    pub(crate) fn build(
        world: &mut World,
        match_setup: MatchSetup,
        packages: &ModePackages,
        players: u32,
        picks: &OptionPicks,
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
        picks: &OptionPicks,
    ) -> Match {
        let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
        Self::restore_target_of(spec, packages, players, picks)
    }

    /// The restore target of `setup`, the setup of `packages` for `players` players, of `picks`.
    fn restore_target_of(
        setup: MatchSetup,
        packages: &ModePackages,
        players: u32,
        picks: &OptionPicks,
    ) -> Match {
        let mut world = World::new();
        Self::build(&mut world, setup, packages, players, picks);
        Match::from_built(world)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::{Match, MatchKind, Release};
    use campfire_common::SegmentSeed;
    use campfire_log::internals::LogCheck;
    use campfire_mode::OptionPicks;
    use campfire_package::ModePackages;
    use campfire_players::Players;
    use campfire_sim::MatchRate;
    use campfire_vision::VisionBook;

    use crate::match_build::MatchBuild;

    /// What an arena's match is built of: its packages, its tick rate, its players, and its map's
    /// vision, the one book a test gives in place of the packages'.
    #[derive(Debug)]
    pub struct ArenaSetup<'a> {
        pub packages: &'a ModePackages,
        pub rate: MatchRate,
        pub players: u32,
        pub vision: VisionBook,
    }

    impl MatchBuild {
        /// The match of `arena`'s packages at its rate, for its players, with every book in place
        /// but the map's vision, which is its own, and no mode installed, as an arena holds it; it
        /// starts with the match's entity alone, spawns the mode's unit types as the mode does
        /// for a test to spawn, and fails its test on any warning it logs.
        pub fn arena(arena: ArenaSetup<'_>) -> Match {
            let ArenaSetup {
                packages,
                rate,
                players,
                vision,
            } = arena;
            let log = LogCheck::start();
            let spec = Self::spec(packages, SegmentSeed::ZERO, rate, players);
            let mut books = packages.books(rate);
            books.replace(vision);
            let mut game = Match::new(spec, books, MatchKind::Rules).with_log(log);
            Release::compile(&mut game, &packages.scripts());
            Players::start_without_mode(&mut game, [], 0);
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
        const fn rate(packages: &ModePackages) -> MatchRate {
            MatchRate::new(packages.manifest().tick_hz.default())
        }
    }
}
