use campfire_protocol::{Applied, InputError, SeedError, SessionLog};
use campfire_runner::Runner;

/// A published session log replayed in a bare `World`, one tick at a time. Recording the logged
/// packets again gives each tick exactly the inputs the server applied. It also checks every chain
/// link and signature again, which a decoded log already passed: the runner's log only takes
/// packets through `record`.
#[derive(Debug)]
pub struct Replay<'a> {
    published: &'a SessionLog,
    runner: Runner,
    /// Scratch for when each recorded input applies.
    applied: Vec<Applied>,
}

impl<'a> Replay<'a> {
    /// The replay with the log's own randomness; an error when the log does not reveal the server
    /// seed its header commits to.
    pub fn new(published: &'a SessionLog) -> Result<Replay<'a>, SeedError> {
        let server_seed = published.revealed_seed().ok_or(SeedError::NotRevealed)?;
        let log = SessionLog::new(published.header().clone())
            .expect("a published log's header starts a log");
        Ok(Replay {
            published,
            runner: Runner::new(log, server_seed)?,
            applied: Vec::new(),
        })
    }

    /// Runs the next tick the log holds; `None` after its last tick.
    pub fn next_tick(&mut self) -> Option<Result<(), InputError>> {
        let tick = self.runner.log().next_tick();
        if tick == self.published.next_tick() {
            return None;
        }
        for packet in self.published.packets_before(tick) {
            if let Err(error) =
                self.runner
                    .record(packet.inputs(), &packet.signature, &mut self.applied)
            {
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
