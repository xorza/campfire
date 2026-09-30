use campfire_protocol::{SeedError, SessionLog};
use campfire_runner::Runner;

/// A published session log replayed in a bare `World`, one tick at a time. Decoding the log
/// checked every chain link and signature; the replay seals its ticks again, which gives each
/// tick exactly the inputs the server applied, with no check done twice.
#[derive(Debug)]
pub struct Replay {
    runner: Runner,
    /// The ticks the log holds.
    ticks: u64,
}

impl Replay {
    /// The replay with the log's own randomness; an error when the log does not reveal the server
    /// seed its header commits to.
    pub fn new(published: SessionLog) -> Result<Replay, SeedError> {
        let server_seed = published.revealed_seed().ok_or(SeedError::NotRevealed)?;
        let ticks = published.next_tick();
        Ok(Replay {
            runner: Runner::new(published.rewound(), server_seed)?,
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
