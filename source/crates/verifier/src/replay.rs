use campfire_protocol::{InputError, SeedError, SessionLog};
use campfire_runner::Runner;

/// A published session log replayed in a bare `World`, one tick at a time. Recording the logged
/// inputs again checks every chain link and gives each tick exactly the inputs the server applied.
#[derive(Debug)]
pub struct Replay<'a> {
    published: &'a SessionLog,
    runner: Runner,
}

impl<'a> Replay<'a> {
    /// The replay with the log's own segment seed; an error when the log does not reveal the seed
    /// its header commits to.
    pub fn new(published: &'a SessionLog) -> Result<Replay<'a>, SeedError> {
        Ok(Replay {
            published,
            runner: Runner::new(published.header().clone(), published.segment_seed()?),
        })
    }

    /// Runs the next tick the log holds; `None` after its last tick.
    pub fn next_tick(&mut self) -> Option<Result<(), InputError>> {
        let tick = self.runner.log().next_tick();
        if tick == self.published.next_tick() {
            return None;
        }
        for input in self.published.logged_before(tick) {
            if let Err(error) = self.runner.record(input) {
                return Some(Err(error));
            }
        }
        self.runner.run_tick();
        Some(Ok(()))
    }

    pub const fn runner(&self) -> &Runner {
        &self.runner
    }
}
