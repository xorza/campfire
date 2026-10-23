use std::path::PathBuf;

use clap::Parser;

use crate::error::SelectionError;
use crate::match_play::PlayRecord;
use crate::play::Play;
use crate::play_selection::PlaySelection;
use crate::probe_arg::ProbeArg;

/// Imports the Zero Hour install `GENERALSZH_DATA` names and checks what it gives.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The directory the import's packages go to, in place of those an earlier run wrote there;
    /// `zh-package` in the home directory when none is given
    pub(crate) out: Option<PathBuf>,
    /// A play to run, for each one named; every play when none is
    #[arg(long = "play", value_enum)]
    pub(crate) plays: Vec<Play>,
    /// A probe of a play's units, `<play>:<period_ms>:<units>`, the units `all`, `player=<n>`
    /// from 1, `team=<n>`, `type=<name>` or `ids=<id>,<id>`; it writes `<play>.probe` in the
    /// probes' directory
    #[arg(long = "probe")]
    pub(crate) probes: Vec<ProbeArg>,
    /// Each play keeps a record: each tick's copy of its state checked, and its log replayed to
    /// the same hashes as it ends, so a determinism bug names its tick; the bot plays then take
    /// about four times as long
    #[arg(long)]
    pub(crate) record: bool,
    /// The directory the probes write to
    #[arg(long, default_value = "target/zero-hour")]
    pub(crate) probe_dir: PathBuf,
}

impl CommandLine {
    /// The directory the packages go to: the one given, or `zh-package` in `home`; none when
    /// neither names one.
    pub(crate) fn out_dir(&self, home: Option<PathBuf>) -> Option<PathBuf> {
        let out = self.out.clone();
        out.or_else(|| home.map(|home| home.join("zh-package")))
    }

    /// The plays it names and their probes; an error when two probes name one play, or a probe
    /// names a play it does not play.
    pub(crate) fn selection(self) -> Result<PlaySelection, SelectionError> {
        let record = if self.record {
            PlayRecord::Kept
        } else {
            PlayRecord::None
        };
        PlaySelection::new(self.plays, self.probes, self.probe_dir, record)
    }
}

#[cfg(test)]
mod tests;
