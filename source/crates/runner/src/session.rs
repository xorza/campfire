use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{Mode, ScriptFailures};
use campfire_content::Fingerprint as PackageFingerprint;
use campfire_package::{ModePackages, PackageStore, RELEASE};
use campfire_protocol::{
    Applied, Fingerprint, InputError, PlayerInput, ServerSeed, SessionLog, SessionTerms, Signature,
};
use campfire_sim::{
    PlayerSlot, SimTick, SimUpdate, StableId, StateHash, StateRegistry, TickInput, TickInputs,
    TickRate,
};
use tracing::warn;

use crate::error::StartError;
use crate::match_build::MatchBuild;

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive; a
/// verifier records a published log's inputs again. Either way each tick applies exactly the
/// inputs the log gives it.
#[derive(Resource, Debug)]
pub struct Session {
    /// The first segment's server seed, secret until `reveal_seed` publishes the log.
    server_seed: ServerSeed,
    state: StateRegistry,
    log: SessionLog,
}

impl Session {
    /// Prepares `world` for the match of `log`'s header, of the mode `packages` holds, at the
    /// header's tick rate and with the randomness of `server_seed`, the first segment's, and the
    /// players' contributions; starts the match; and inserts the session, which records into
    /// `log` from its first tick. An error when the terms name another release, mode or
    /// dependencies than this release and `packages`, a tick rate outside the mode's range, or
    /// `server_seed` is not the first segment's seed of the chain the header commits to, or when
    /// the packages do not load into the match.
    pub fn start(
        world: &mut World,
        log: SessionLog,
        server_seed: ServerSeed,
        packages: &ModePackages,
    ) -> Result<(), StartError> {
        assert_eq!(log.next_tick(), 0, "a session starts before its first tick");
        let header = log.header();
        let terms = &header.terms;
        if terms.release != RELEASE {
            return Err(StartError::OtherRelease(terms.release.clone()));
        }
        if terms.mode != in_terms(packages.fingerprint()) {
            return Err(StartError::OtherMode);
        }
        if !packages
            .dependency_fingerprints()
            .map(in_terms)
            .eq(terms.dependencies.iter().copied())
        {
            return Err(StartError::OtherDependencies);
        }
        let hz = terms.tick_hz;
        if !packages.manifest().tick_hz.contains(hz) {
            return Err(StartError::TickRate(hz));
        }
        let seed = header
            .segment_seed(0, &server_seed)
            .map_err(StartError::Seed)?;
        SimUpdate::prepare(world, seed, TickRate::new(hz));
        let mut schedule = SimUpdate::schedule();
        let mut state = StateRegistry::new();
        let players = u32::try_from(header.players.len()).expect("the log counts players in u32");
        MatchBuild::run(packages, world, &mut schedule, &mut state, players)?;
        world.add_schedule(schedule);
        Mode::start(world).map_err(StartError::MatchStart)?;
        world.insert_resource(Session {
            server_seed,
            state,
            log,
        });
        Ok(())
    }

    /// The mode and the dependencies `terms` name, from `store`, as a verifier holds them.
    pub fn packages(
        store: &PackageStore,
        terms: &SessionTerms,
    ) -> Result<ModePackages, StartError> {
        let dependencies: Vec<_> = terms
            .dependencies
            .iter()
            .map(|&each| of_package(each))
            .collect();
        ModePackages::from_store(store, of_package(terms.mode), &dependencies)
            .map_err(StartError::Packages)
    }

    /// The mode of `packages` as session terms name it.
    pub const fn mode_in_terms(packages: &ModePackages) -> Fingerprint {
        in_terms(packages.fingerprint())
    }

    /// The dependencies of `packages` as session terms name them, in the order of their names.
    pub fn dependencies_in_terms(packages: &ModePackages) -> Vec<Fingerprint> {
        packages.dependency_fingerprints().map(in_terms).collect()
    }

    /// Logs a player's packet before the next tick; see `SessionLog::record`.
    pub fn record<'a, I>(
        &mut self,
        inputs: I,
        signature: &Signature,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = PlayerInput<'a>>,
        I::IntoIter: Clone,
    {
        self.log.record(inputs, signature, applied)
    }

    /// Seals the next tick in the log of the session in `world` and runs it with the inputs
    /// applied in it, and logs each script call of the tick that failed.
    pub fn run_tick(world: &mut World) {
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            debug_assert_eq!(
                world.resource::<SimTick>().start().get(),
                session.log.next_tick(),
                "the sim and the log are at the same tick"
            );
            let mut inputs = world.resource_mut::<TickInputs>();
            // The log's slot type becomes the sim's here, where the log's inputs enter the sim.
            for input in session.log.seal_tick() {
                inputs.push(TickInput {
                    slot: PlayerSlot::new(input.slot.get()),
                    payload: input.payload,
                });
            }
        });
        let tick = world.resource::<SimTick>().start().get();
        world.run_schedule(SimUpdate);
        if let Some(failures) = world.get_non_send::<ScriptFailures>() {
            for failure in failures.get() {
                warn!(
                    tick,
                    unit = failure.unit.map(StableId::get),
                    hook = ?failure.hook,
                    error = %failure.error,
                    "a script call failed and changed nothing"
                );
            }
        }
    }

    /// Publishes the log's segment by adding the server seed.
    pub fn reveal_seed(&mut self) {
        self.log.reveal_seed(self.server_seed);
    }

    pub fn state_hash(&self, world: &World) -> StateHash {
        self.state.hash(world)
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }
}

/// A package's fingerprint as the session terms name it: the terms and the packages each own a
/// fingerprint type, and they meet here.
const fn in_terms(fingerprint: PackageFingerprint) -> Fingerprint {
    Fingerprint::new(*fingerprint.as_bytes())
}

/// The fingerprint the session terms name, as the package store holds packages by it.
const fn of_package(fingerprint: Fingerprint) -> PackageFingerprint {
    PackageFingerprint::new(*fingerprint.as_bytes())
}
