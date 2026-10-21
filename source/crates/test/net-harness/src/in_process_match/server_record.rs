use campfire_harness::HashTrail;
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;
use campfire_runner::Replay;

/// What an in-process server's session published: its log's file, and its hash after each tick it
/// played.
#[derive(Debug)]
pub struct ServerRecord {
    pub file: Vec<u8>,
    pub live: HashTrail,
}

impl ServerRecord {
    /// The log, as its file decodes it.
    pub fn log(&self) -> SessionLog {
        SessionLog::decode(&self.file).expect("a log decodes from its file")
    }

    /// The hashes of the log's replay on `packages`.
    pub fn replayed(&self, packages: &ModePackages) -> HashTrail {
        let mut replay = Replay::of(self.log(), packages).expect("the log's match starts");
        HashTrail::of_replay(&mut replay).expect("the log replays to what it records")
    }

    /// Asserts that the log replays on `packages` to the hashes the server played.
    pub fn assert_replays(&self, packages: &ModePackages) {
        self.live.assert_same(&self.replayed(packages));
    }
}
