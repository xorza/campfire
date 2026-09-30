use campfire_content::PackageStore;
use campfire_protocol::{SeedError, SessionLog};
use campfire_runner::{ModePackages, RELEASE, Runner, StartError};

/// A published session log replayed in a bare `World`, one tick at a time, with the packages its
/// terms name. Decoding the log checked every chain link and signature; the replay seals its
/// ticks again, which gives each tick exactly the inputs the server applied, with no check done
/// twice.
#[derive(Debug)]
pub struct Replay {
    runner: Runner,
    /// The ticks the log holds.
    ticks: u64,
}

impl Replay {
    /// The replay with the log's own randomness and the mode its terms name, from `store`; an
    /// error when the log does not reveal the server seed its header commits to, names another
    /// engine release, or a mode or dependency `store` does not hold, or the mode does not run
    /// at the log's tick rate.
    pub fn new(published: SessionLog, store: &PackageStore) -> Result<Replay, StartError> {
        let server_seed = published
            .revealed_seed()
            .ok_or(StartError::Seed(SeedError::NotRevealed))?;
        let terms = &published.header().terms;
        if terms.release != RELEASE {
            return Err(StartError::OtherRelease(terms.release.clone()));
        }
        let packages = ModePackages::from_store(store, terms.mode, &terms.dependencies)?;
        let ticks = published.next_tick();
        Ok(Replay {
            runner: Runner::new(published.rewound(), server_seed, &packages)?,
            ticks,
        })
    }

    /// Runs the next tick the log holds; `false` after its last tick.
    pub fn run_tick(&mut self) -> bool {
        if self.runner.log().next_tick() == self.ticks {
            return false;
        }
        self.runner.run_tick();
        true
    }

    pub const fn runner(&self) -> &Runner {
        &self.runner
    }
}
