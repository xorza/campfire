use std::path::PathBuf;

use crate::error::SelectionError;
use crate::match_play::PlayRecord;
use crate::play::Play;
use crate::probe_arg::{PlayProbe, ProbeArg};

/// The plays a run plays, every play when it names none, and the probe of each play it probes,
/// each of a play it plays, one for each play, writing into `probe_dir`, and whether its plays
/// keep a record.
#[derive(Debug)]
pub(crate) struct PlaySelection {
    plays: Vec<Play>,
    probes: Vec<ProbeArg>,
    probe_dir: PathBuf,
    record: PlayRecord,
}

impl PlaySelection {
    /// The selection of `plays` and `probes`; an error when two probes name one play, or a probe
    /// names a play the run does not play.
    pub(crate) fn new(
        plays: Vec<Play>,
        probes: Vec<ProbeArg>,
        probe_dir: PathBuf,
        record: PlayRecord,
    ) -> Result<Self, SelectionError> {
        let selection = Self {
            plays,
            probes,
            probe_dir,
            record,
        };
        for (at, probe) in selection.probes.iter().enumerate() {
            if selection.probes[..at]
                .iter()
                .any(|earlier| earlier.play == probe.play)
            {
                return Err(SelectionError::Twice(probe.play));
            }
            if !selection.runs(probe.play) {
                return Err(SelectionError::Unplayed(probe.play));
            }
        }
        Ok(selection)
    }

    /// Whether the run plays `play`.
    pub(crate) fn runs(&self, play: Play) -> bool {
        self.plays.is_empty() || self.plays.contains(&play)
    }

    /// Whether its plays keep a record.
    pub(crate) const fn record(&self) -> PlayRecord {
        self.record
    }

    /// The probe of `play`, if the run probes it.
    pub(crate) fn probe(&self, play: Play) -> Option<PlayProbe> {
        let arg = self.probes.iter().find(|probe| probe.play == play)?;
        Some(arg.of_play(&self.probe_dir))
    }
}
