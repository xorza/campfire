use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use campfire_store::DurableFile;

use crate::error::CheckError;

/// The directory of one run, below the run root, named for the run's start in UTC, to the
/// millisecond: `2026-09-30T17-45-12.123Z`. A run never touches another run's directory.
#[derive(Debug)]
pub(crate) struct RunDir {
    path: PathBuf,
}

impl RunDir {
    /// Creates the directory of a run that started at `start` below `root`, and `root` if it
    /// does not exist; an error if the directory exists.
    pub(crate) fn create(root: &Path, start: SystemTime) -> Result<RunDir, CheckError> {
        let path = root.join(RunDir::name(start));
        DurableFile::create_dir_all(root).map_err(CheckError::Write)?;
        DurableFile::create_new_dir(&path).map_err(CheckError::RunDir)?;
        Ok(RunDir { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn name(start: SystemTime) -> String {
        let since = start
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970");
        let seconds = since.as_secs();
        let [year, month, day] = RunDir::civil(seconds / 86_400);
        let time = seconds % 86_400;
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}.{:03}Z",
            time / 3600,
            time / 60 % 60,
            time % 60,
            since.subsec_millis()
        )
    }

    /// The year, month and day of `days` after 1970-01-01, in the proleptic Gregorian calendar:
    /// days count in 400-year eras of 146 097 days, and each year from March, so February's leap
    /// day ends it.
    const fn civil(days: u64) -> [u64; 3] {
        let from_march = days + 719_468;
        let era = from_march / 146_097;
        let of_era = from_march % 146_097;
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_from_march = (5 * of_year + 2) / 153;
        let day = of_year - (153 * month_from_march + 2) / 5 + 1;
        let month = if month_from_march < 10 {
            month_from_march + 3
        } else {
            month_from_march - 9
        };
        let year = era * 400 + year_of_era + if month <= 2 { 1 } else { 0 };
        [year, month, day]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use campfire_store::{DurableCreateError, PathError, Scratch};

    use super::*;

    fn at(millis: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(millis)
    }

    #[test]
    fn each_run_gets_a_directory_of_its_own_named_for_its_start() {
        assert_eq!(RunDir::name(at(0)), "1970-01-01T00-00-00.000Z");
        // 1 790 000 000 s is 20 717 days and 51 200 s: 14 h 13 min 20 s into 2026-09-21. 2026
        // begins 20 454 days after 1970, so the day is its 264th.
        assert_eq!(
            RunDir::name(at(1_790_000_000_123)),
            "2026-09-21T14-13-20.123Z"
        );
        // The leap day of 2000, 11 016 days after 1970, and the last millisecond of that day.
        assert_eq!(
            RunDir::name(at(11_016 * 86_400_000 + 86_399_999)),
            "2000-02-29T23-59-59.999Z"
        );

        // Below a directory that goes when the test ends, passed or failed.
        let scratch = Scratch::new();
        let root = scratch.path("runs");
        let first = RunDir::create(&root, at(1_000)).unwrap();
        let kept = Path::new("runs")
            .join(RunDir::name(at(1_000)))
            .join("server.jsonl");
        scratch.write(&kept, "kept");
        let second = RunDir::create(&root, at(2_000)).unwrap();
        assert_ne!(first.path(), second.path());
        assert_eq!(scratch.read_text(&kept), "kept");
        assert!(matches!(
            RunDir::create(&root, at(1_000)),
            Err(CheckError::RunDir(PathError {
                error: DurableCreateError::Exists,
                ..
            }))
        ));
    }
}
