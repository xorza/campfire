use std::io::Write;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use campfire_capabilities::{MatchProbe, ProbeSpec, ProbeUnits};
use campfire_common::Ticks;
use campfire_host::{ReportDir, ReportFile};
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_protocol::SessionLog;
use campfire_runner::Replay;
use campfire_sim::MatchRate;
use campfire_store::{DurableDir, DurableFile, InputFile};
use campfire_units::UnitTypeBook;
use tracing::info;

use crate::report_replay::error::ReportReplayError;

pub(crate) mod error;

/// A report as its replay reads it: its facts, its log, and the mode the facts name, loaded from
/// the directory the client read it from (bug-reports.md, A report replays to its tick).
#[derive(Debug)]
pub(crate) struct ReportReplay {
    facts: ReportFile,
    log: SessionLog,
    packages: ModePackages,
}

/// What a replay tables: the units at the report's tick, and, of a probe, each of its periods
/// before it.
#[derive(Debug)]
pub(crate) struct ReportTables {
    pub(crate) at: MatchProbe,
    pub(crate) periods: Option<MatchProbe>,
}

/// A probe of a report's replay: what it probes, and the directory its file goes in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportProbe {
    pub spec: ProbeSpec,
    pub dir: PathBuf,
}

/// Replays the report in `report` to its tick and writes the table of its units there to `out`;
/// with `probe`, writes its tables of each period before the tick to
/// `<session>-<tick>.probe` in its directory, and logs the file. An error when the report does
/// not replay, or a table does not write.
pub fn replay_report(
    report: &Path,
    probe: Option<&ReportProbe>,
    mut out: impl Write,
) -> Result<(), ReportReplayError> {
    let replay = ReportReplay::open(&ReportDir::at(report.to_owned()))?;
    let name = replay.name();
    let tables = replay.tables(probe.map(|probe| &probe.spec))?;
    if let (Some(probe), Some(periods)) = (probe, &tables.periods) {
        DurableDir::create_all(&probe.dir).map_err(ReportReplayError::ProbeDir)?;
        let file = probe.dir.join(format!("{name}.probe"));
        DurableFile::write(&file, periods.text().as_bytes())
            .map_err(ReportReplayError::ProbeFile)?;
        info!(file = %file.display(), "the probe's tables");
    }
    out.write_all(tables.at.text().as_bytes())
        .map_err(ReportReplayError::Print)
}

impl ReportReplay {
    /// The report in `dir`; an error when its facts or its log do not read, or its mode does not
    /// load.
    pub(crate) fn open(dir: &ReportDir) -> Result<Self, ReportReplayError> {
        let facts = dir.read_facts().map_err(ReportReplayError::Facts)?;
        let bytes = InputFile::read(&dir.log(), SessionLog::MAX_FILE_LEN)
            .map_err(ReportReplayError::ReadLog)?;
        let log = SessionLog::decode(&bytes).map_err(ReportReplayError::Decode)?;
        let packages = ModePackages::at(&PackageDir::new(&facts.mode_dir), Some(&facts.map))
            .map_err(ReportReplayError::Packages)?;
        Ok(Self {
            facts,
            log,
            packages,
        })
    }

    /// The report's name in a file of its tables: its session's first digits and its tick.
    pub(crate) fn name(&self) -> String {
        let session = self.facts.session.to_string();
        format!("{}-{}", &session[..8], self.facts.tick)
    }

    /// Replays the log through the report's tick, the state its client drew, and tables the
    /// units there, those of `probe` when given, else all, with `probe`'s tables of each period before
    /// it; an error when the log does not reach the tick, does not replay on the mode, or the
    /// probe names what the match has not.
    pub(crate) fn tables(
        self,
        probe: Option<&ProbeSpec>,
    ) -> Result<ReportTables, ReportReplayError> {
        let Self {
            facts,
            log,
            packages,
        } = self;
        let (tick, end) = (facts.tick, log.next_tick());
        if tick >= end {
            return Err(ReportReplayError::PastLog { tick, end });
        }
        let mut replay = Replay::of(log, &packages).map_err(ReportReplayError::Start)?;
        let game = replay.game();
        let types = game.get_resource::<UnitTypeBook>();
        let hz = game.resource::<MatchRate>().hz();
        let probed = |spec: &ProbeSpec| spec.probe(types, hz).map_err(ReportReplayError::Probe);
        let mut periods = probe.map(probed).transpose()?;
        let mut at = match probe {
            Some(spec) => probed(spec)?,
            None => MatchProbe::new(NonZeroU64::MIN, ProbeUnits::All),
        };
        replay
            .run_to(tick.after(Ticks::ONE), |world| {
                if let Some(periods) = &mut periods {
                    periods.record(world);
                }
            })
            .map_err(ReportReplayError::Replay)?;
        at.table(replay.game());
        Ok(ReportTables { at, periods })
    }
}

#[cfg(test)]
mod tests;
