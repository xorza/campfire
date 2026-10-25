use std::path::{Path, PathBuf};
use std::{env, thread};

use bevy_ecs::world::World;
use campfire_capabilities::{MatchProbe, ProbeSpec};
use campfire_common::ErrorReport;
use campfire_protocol::SessionId;
use campfire_sim::MatchRate;
use campfire_store::{DurableDir, DurableFile};
use campfire_units::UnitTypeBook;

use crate::error::ProbeWriteError;

/// A probe of a match, and the file it writes.
#[derive(Debug)]
pub struct ProbeFile {
    pub probe: MatchProbe,
    pub file: PathBuf,
}

impl ProbeFile {
    /// The variable that asks every fixed match for a probe.
    const VAR: &'static str = "CAMPFIRE_PROBE";

    /// The probe `CAMPFIRE_PROBE` asks of `game`, a match of the session `session`, which writes
    /// `target/probe/<test>-<session>.probe`, its test the thread it runs on, as libtest names
    /// it, its session the first digits of its id; none with no variable. A variable that does
    /// not read, or a probe the match refuses, is a test's misuse, and fails it.
    pub(crate) fn of_env(game: &World, session: SessionId) -> Option<Self> {
        let text = env::var(Self::VAR).ok()?;
        let spec: ProbeSpec = text
            .parse()
            .unwrap_or_else(|error| panic!("{}={text}: {}", Self::VAR, ErrorReport::of(&error)));
        let types = game.get_resource::<UnitTypeBook>();
        let hz = game.resource::<MatchRate>().hz();
        let probe = spec
            .probe(types, hz)
            .unwrap_or_else(|error| panic!("{}={text}: {}", Self::VAR, ErrorReport::of(&error)));
        let thread = thread::current();
        let test = thread.name().unwrap_or("main").replace("::", "-");
        let session = &session.to_string()[..8];
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../target/probe"
        ));
        Some(Self {
            probe,
            file: dir.join(format!("{test}-{session}.probe")),
        })
    }

    /// Writes what the probe wrote to its file, its directory made if it is missing.
    pub fn write(&self) -> Result<(), ProbeWriteError> {
        if let Some(dir) = self.file.parent() {
            DurableDir::create_all(dir)?;
        }
        DurableFile::write(&self.file, self.probe.text().as_bytes())?;
        Ok(())
    }
}
