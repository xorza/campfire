use std::path::Path;

use campfire_capabilities::internals::Players;
use campfire_capabilities::{Match, MatchSpec, ModeInputs};
use campfire_common::SegmentSeed;
use campfire_log::internals::LogCheck;
use campfire_package::{ModePackages, PackageDir};
use campfire_sim::TickRate;

use crate::harness::harness_world::{ArenaFailures, ArenaPackages};
use crate::match_build::MatchBuild;

/// The matches the harness builds for the tests and benches of every crate, each failing its
/// test on any warning it logs.
pub trait HarnessMatchInternals: Sized {
    /// An arena: the match of the mode at `dir`, on its only map, at `rate`, for `players`
    /// players, with every book in place, the mode's own among them, and no mode installed. Its
    /// script never runs, so no `calc_damage` weighs a hit and no map unit spawns, and its match
    /// starts with the match's entity alone. A test places its units by hand and drives their
    /// actions, as the packages hold them.
    fn arena(dir: &Path, rate: TickRate, players: u32) -> Self;

    /// A restore target: the match of `packages` for `players` players at the mode's default
    /// rate, with its books in place and no unit, to restore snapshots into, as a session that
    /// loads a save will: the books come from the packages, the state from the snapshot.
    fn restore_target(packages: &ModePackages, players: u32) -> Self;

    /// The restore target `restore_target` builds, its schedule built with no automatic sync
    /// points, so its build panics on every pair of systems that only a sync point keeps in
    /// order, naming them.
    fn restore_target_without_sync_points(packages: &ModePackages, players: u32) -> Self;
}

impl HarnessMatchInternals for Match {
    fn arena(dir: &Path, rate: TickRate, players: u32) -> Match {
        let log = LogCheck::start();
        let mode = PackageDir::new(dir);
        let map = ModePackages::choose_map(&mode, None).unwrap();
        let packages = ModePackages::from_package_dir(&mode, &map).unwrap();
        let seed = SegmentSeed::new([0; 32]);
        let mut game = Match::new(MatchBuild::spec(&packages, seed, rate, players)).with_log(log);
        let ModeInputs { books, .. } = MatchBuild::books(&packages, &mut game);
        books.install(&mut game);
        Players::start_without_mode(&mut game, [], 0);
        game.insert_non_send(ArenaPackages(packages));
        game.insert_non_send(ArenaFailures::default());
        game
    }

    fn restore_target(packages: &ModePackages, players: u32) -> Match {
        let log = LogCheck::start();
        let spec = MatchBuild::spec(packages, ZERO_SEED, rate_of(packages), players);
        restore_target_of(log, packages, players, spec)
    }

    fn restore_target_without_sync_points(packages: &ModePackages, players: u32) -> Match {
        let log = LogCheck::start();
        let spec = MatchBuild::spec(packages, ZERO_SEED, rate_of(packages), players);
        restore_target_of(log, packages, players, spec.without_sync_points())
    }
}

/// The seed of a match whose randomness no test reads.
const ZERO_SEED: SegmentSeed = SegmentSeed::new([0; 32]);

/// The default tick rate of the mode `packages` holds.
fn rate_of(packages: &ModePackages) -> TickRate {
    TickRate::new(packages.manifest().tick_hz.default())
}

/// A restore target of `spec`, the spec of `packages` for `players` players, which fails its test
/// on any warning `log` takes.
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
