use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use campfire_capabilities::{Books, ProbeSpec};
use campfire_harness::ProbeFile;
use campfire_units::UnitTypeBook;
use clap::ValueEnum;

use crate::error::ProbeArgError;
use crate::failure::CheckFailure;
use crate::play::Play;

/// A probe the command line asks of a play, `<play>:<period_ms>:<units>`: the play, and the
/// probe as the harness reads its text (02-engine-core.md, One probe of a match).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeArg {
    pub(crate) play: Play,
    pub(crate) spec: ProbeSpec,
}

/// A probe of one play: what it asks, and the file it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayProbe {
    pub(crate) spec: ProbeSpec,
    pub(crate) file: PathBuf,
}

impl ProbeArg {
    /// The probe of the play it names, writing `<play>.probe` in `dir`.
    pub(crate) fn of_play(&self, dir: &Path) -> PlayProbe {
        PlayProbe {
            spec: self.spec.clone(),
            file: dir.join(format!("{}.probe", self.play)),
        }
    }
}

impl PlayProbe {
    /// The probe in a match of `books` at `rate` ticks a second, before the play runs; a failure
    /// for a probe the match refuses.
    pub(crate) fn resolve(
        self,
        books: &Books,
        rate: NonZeroU32,
    ) -> Result<ProbeFile, CheckFailure> {
        let probe = self.spec.probe(books.get::<UnitTypeBook>(), rate);
        Ok(ProbeFile {
            probe: probe.map_err(CheckFailure::Probe)?,
            file: self.file,
        })
    }
}

impl FromStr for ProbeArg {
    type Err = ProbeArgError;

    fn from_str(text: &str) -> Result<Self, ProbeArgError> {
        let (play, spec) = text.split_once(':').ok_or(ProbeArgError::Shape)?;
        Ok(Self {
            play: Play::from_str(play, false).map_err(|_| ProbeArgError::Play)?,
            spec: spec.parse()?,
        })
    }
}
